use clap::Parser;
use config::Config as AppConfig; // Renamed to avoid conflict with our Config struct
use csv; // For TSV parsing/writing
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::Client;
use rusqlite::params;
use rusqlite::Connection;
use serde::Deserialize;
use std::collections::HashMap;
use indicatif::{ProgressBar, ProgressStyle};
use std::io::{BufRead, BufReader};
use std::error::Error;
use std::fs;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct CliArgs {
    #[clap(short, long, value_parser, default_value = "config.toml")]
    config_file_path: String,
}

#[derive(Debug, Deserialize)]
struct Config {
    input_tsv_file_path: String,
    output_tsv_file_path: Option<String>,
    sqlite_db_path: Option<String>,
    sqlite_table_name: Option<String>,
    instance_old: InstanceConfig,
    instance_new: InstanceConfig,
}

#[derive(Debug, Deserialize)]
struct InstanceConfig {
    base_url: String,
    api_token: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
struct JellyfinUser {
    id: String,
    name: String,
    // We can add other fields here if needed later, like ServerId
}

// Placeholder for TSV record structure based on the provided headers
// 0|DateCreated|DATETIME|1||0
// 1|UserId|TEXT|0||0
// 2|ItemId|TEXT|0||0
// 3|ItemType|TEXT|0||0
// 4|ItemName|TEXT|0||0
// 5|PlaybackMethod|TEXT|0||0
// 6|ClientName|TEXT|0||0
// 7|DeviceName|TEXT|0||0
// 8|PlayDuration|INT|0||0
#[derive(Debug, Deserialize, serde::Serialize)]
struct TsvRecord {
    #[serde(rename = "DateCreated")]
    date_created: String,
    #[serde(rename = "UserId")]
    user_id: String,
    #[serde(rename = "ItemId")]
    item_id: String,
    #[serde(rename = "ItemType")]
    item_type: String,
    #[serde(rename = "ItemName")]
    item_name: String,
    #[serde(rename = "PlaybackMethod")]
    playback_method: String,
    #[serde(rename = "ClientName")]
    client_name: String,
    #[serde(rename = "DeviceName")]
    device_name: String,
    #[serde(rename = "PlayDuration")]
    play_duration: String, // Reading as string initially, can be parsed to INT if needed
}

fn load_config(config_path_str: &str) -> Result<Config, config::ConfigError> {
    let builder = AppConfig::builder();

    // Attempt to load the specified/default config file
    let primary_config_builder =
        builder.add_source(config::File::with_name(config_path_str).required(true));

    match primary_config_builder.build() {
        Ok(settings) => {
            println!("Successfully built configuration from: {}", config_path_str);
            settings.try_deserialize::<Config>()
        }
        Err(e) => {
            eprintln!(
                "Failed to load configuration from '{}': {}. Attempting fallback 'config.example.toml'.",
                config_path_str, e
            );
            // If the primary config failed (e.g. not found or malformed), try the example config as a fallback.
            let fallback_builder = AppConfig::builder(); // Create a new builder for fallback
            fallback_builder
                .add_source(config::File::with_name("config.example.toml").required(true))
                .build()?
                .try_deserialize::<Config>()
        }
    }
}

async fn fetch_users_from_instance(
    instance_config: &InstanceConfig,
    client: &Client,
) -> Result<Vec<JellyfinUser>, Box<dyn Error>> {
    let url = format!("{}/Users", instance_config.base_url);

    let mut headers = HeaderMap::new();
    let token_value = format!("MediaBrowser Token=\"{}\"", instance_config.api_token);
    match HeaderValue::from_str(&token_value) {
        Ok(header_val) => {
            headers.insert(AUTHORIZATION, header_val);
        }
        Err(e) => {
            return Err(Box::new(e) as Box<dyn Error>);
        }
    }
    // Jellyfin also often requires X-Emby-Token
    match HeaderValue::from_str(&instance_config.api_token) {
        // Use the raw token for X-Emby-Token
        Ok(header_val) => {
            headers.insert("X-Emby-Token", header_val);
        }
        Err(e) => {
            return Err(Box::new(e) as Box<dyn Error>);
        }
    }

    println!("Fetching users from: {}", url);

    let response = client.get(&url).headers(headers).send().await?;

    let status = response.status(); // Store status before consuming response
    if !status.is_success() {
        let error_text = response.text().await?; // Consume response body for error message
        return Err(format!(
            "API request failed for {}: {} - {}",
            url, status, error_text
        )
        .into());
    }

    let users: Vec<JellyfinUser> = response.json().await?; // Consume response body for successful deserialization
    Ok(users)
}

fn create_user_id_map(
    old_users: &[JellyfinUser],
    new_users: &[JellyfinUser],
) -> HashMap<String, String> {
    let mut user_id_map = HashMap::new();
    // Create a quick lookup for new users by name to new user's ID
    let new_users_by_name_to_id: HashMap<&String, &String> =
        new_users.iter().map(|u| (&u.name, &u.id)).collect();

    println!("\nCreating User ID Map:");
    for old_user in old_users {
        if let Some(new_id) = new_users_by_name_to_id.get(&old_user.name) {
            user_id_map.insert(old_user.id.clone(), (*new_id).clone());
            println!(
                "  Mapping user '{}': Old ID '{}' -> New ID '{}'",
                old_user.name, old_user.id, new_id
            );
        } else {
            println!(
                "  User '{}' (ID: '{}') from old instance not found by name in new instance. No mapping created.",
                old_user.name, old_user.id
            );
        }
    }
    if user_id_map.is_empty() {
        println!(
            "  No users were found with matching names across instances. User ID map is empty."
        );
    }
    user_id_map
}

fn check_and_insert_record_into_db(
    conn: &Connection,
    table_name: &str,
    record: &TsvRecord,
) -> Result<bool, rusqlite::Error> {
    // Returns true if inserted, false if skipped (duplicate)
    // Check if the exact record already exists
    let check_query = format!(
        "SELECT EXISTS(SELECT 1 FROM {} WHERE \
        DateCreated = ?1 AND \
        UserId = ?2 AND \
        ItemId = ?3 AND \
        ItemType = ?4 AND \
        ItemName = ?5 AND \
        PlaybackMethod = ?6 AND \
        ClientName = ?7 AND \
        DeviceName = ?8 AND \
        PlayDuration = ?9 \
        LIMIT 1)",
        table_name
    );
    let mut stmt_check = conn.prepare_cached(&check_query)?;
    let exists: bool = stmt_check.query_row(
        params![
            record.date_created,
            record.user_id,
            record.item_id,
            record.item_type,
            record.item_name,
            record.playback_method,
            record.client_name,
            record.device_name,
            record.play_duration,
        ],
        |row| row.get(0),
    )?;

    if exists {
        Ok(false) // Record already exists, skip insertion
    } else {
        // Insert the record
        let insert_query = format!(
            "INSERT INTO {} (DateCreated, UserId, ItemId, ItemType, ItemName, PlaybackMethod, ClientName, DeviceName, PlayDuration) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            table_name
        );
        let mut stmt_insert = conn.prepare_cached(&insert_query)?;
        stmt_insert.execute(params![
            record.date_created,
            record.user_id,
            record.item_id,
            record.item_type,
            record.item_name,
            record.playback_method,
            record.client_name,
            record.device_name,
            record.play_duration,
        ])?;
        Ok(true) // Record was inserted
    }
}

async fn process_tsv_file(
    config: &Config,
    user_id_map: &HashMap<String, String>,
) -> Result<(), Box<dyn Error>> {
    println!("\nStarting TSV/DB processing...");
    println!("Input TSV file: {}", config.input_tsv_file_path);

    // Count lines for progress bar
    let file_for_counting = fs::File::open(&config.input_tsv_file_path)?;
    let reader_for_counting = BufReader::new(file_for_counting);
    let total_lines = reader_for_counting.lines().count() as u64;

    let pb = ProgressBar::new(total_lines);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) - {msg}")
        .expect("Progress bar style template is invalid")
        .progress_chars("#>-"));
    pb.set_message("Processing records...");


    if user_id_map.is_empty() {
        pb.println("User ID map is empty. No UserID replacements will be made, but data will be processed to configured outputs.");
    }

    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(false) // Input TSV does not have headers
        .from_path(&config.input_tsv_file_path)?;

    // Setup TSV Writer if path is configured
    let mut tsv_wtr: Option<csv::Writer<fs::File>> = None;
    if let Some(ref path_str) = config.output_tsv_file_path {
        pb.println(format!("TSV Output will be written to: {}", path_str));
        tsv_wtr = Some(
            csv::WriterBuilder::new()
                .delimiter(b'\t')
                // No headers for TSV output, matching input
                .from_path(path_str)?,
        );
    } else {
        pb.println("TSV Output is not configured.");
    }

    // Setup SQLite Connection if path is configured
    let mut sqlite_conn: Option<Connection> = None;
    let mut records_inserted_sqlite = 0u32; // Counter for SQLite inserts
    let mut records_skipped_sqlite = 0u32; // Counter for skipped duplicate SQLite records

    if let Some(ref db_path_str) = config.sqlite_db_path {
        pb.println(format!("SQLite Output will be written to: {}", db_path_str));
        let conn = Connection::open(db_path_str)?;
        // Start a transaction for bulk inserts
        match conn.execute_batch("BEGIN IMMEDIATE TRANSACTION;") {
            Ok(_) => pb.println("SQLite transaction started."),
            Err(e) => {
                pb.suspend(|| {
                    eprintln!("Failed to start SQLite transaction: {}", e);
                });
                // Potentially return Err here or handle as non-critical if SQLite is optional
                return Err(Box::new(e));
            }
        }
        sqlite_conn = Some(conn);
    } else {
        pb.println("SQLite Output is not configured.");
    }
    let sqlite_table_name = config
        .sqlite_table_name
        .as_deref()
        .unwrap_or("PlaybackActivity");

    if tsv_wtr.is_none() && sqlite_conn.is_none() {
        pb.println("\nWarning: No output (TSV or SQLite) is configured. The application will process data but not save it.");
        // Early exit or just let it run through without outputting might be desired.
        // For now, it will run through, which is fine for UserID mapping summary.
    }

    let mut records_processed = 0;
    let mut records_changed = 0;
    // Old_ID -> (New_ID, Count of changes for this Old_ID)
    let mut changes_summary: HashMap<String, (String, u32)> = HashMap::new();

    for result in rdr.deserialize() {
        let mut record: TsvRecord = result?;
        records_processed += 1;
        pb.inc(1);

        // Check if the current record's user_id is in our map
        if let Some(new_user_id) = user_id_map.get(&record.user_id) {
            let original_old_user_id = record.user_id.clone(); // Keep a copy of the original old ID for summary
            record.user_id = new_user_id.clone(); // Update the record
            records_changed += 1;

            // Update summary: old_id -> (new_id, count)
            let (_new_id_in_summary, count) = changes_summary
                .entry(original_old_user_id)
                .or_insert_with(|| (new_user_id.clone(), 0));
            *count += 1;
        }

        // Write to TSV if configured
        if let Some(ref mut wtr_instance) = tsv_wtr {
            wtr_instance.serialize(&record)?;
        }

        // Write to SQLite if configured
        if let Some(ref conn_instance) = sqlite_conn {
            match check_and_insert_record_into_db(conn_instance, sqlite_table_name, &record) {
                Ok(inserted) => {
                    if inserted {
                        records_inserted_sqlite += 1;
                    } else {
                        records_skipped_sqlite += 1;
                    }
                }
                Err(e) => {
                    pb.suspend(|| {
                        eprintln!(
                            "Error checking/inserting record into SQLite: {:?}. Error: {}. Transaction will be rolled back.",
                            record, e
                        );
                    });
                    // Attempt to rollback before propagating the error
                    if let Err(rb_err) = conn_instance.execute_batch("ROLLBACK;") {
                        eprintln!("Failed to rollback SQLite transaction: {}", rb_err);
                    }
                    return Err(Box::new(e)); // Propagate the original error
                }
            }
        }
    }
    pb.finish_with_message("Record processing loop finished.");

    if let Some(ref mut wtr_instance) = tsv_wtr {
        wtr_instance.flush()?; // Ensure all TSV data is written
    }

    if let Some(conn_instance) = &sqlite_conn {
        match conn_instance.execute_batch("COMMIT;") {
            Ok(_) => println!("SQLite transaction committed successfully."),
            Err(e) => {
                eprintln!(
                    "Failed to commit SQLite transaction: {}. Attempting rollback.",
                    e
                );
                if let Err(rb_err) = conn_instance.execute_batch("ROLLBACK;") {
                    eprintln!("Failed to rollback SQLite transaction: {}", rb_err);
                }
                // Propagate the commit error
                return Err(Box::new(e));
            }
        }
    }

    println!("\nTSV Processing Summary:");
    println!("  Total records processed: {}", records_processed);
    println!("  Total records with UserID changed: {}", records_changed);
    if config.sqlite_db_path.is_some() {
        // Only print SQLite stats if it was configured
        println!(
            "  Total records inserted into SQLite: {}",
            records_inserted_sqlite
        );
        println!(
            "  Total duplicate records skipped in SQLite: {}",
            records_skipped_sqlite
        );
    }
    if !changes_summary.is_empty() {
        println!("  Changes per User ID (Old ID -> New ID: Count of lines changed in TSV/for DB):");
        for (old_id, (new_id, count)) in changes_summary {
            println!("    '{}' -> '{}': {} changes", old_id, new_id, count);
        }
    } else if records_changed > 0 {
        // This case should ideally not be hit if logic is correct
        println!(
            "  Some records were changed, but detailed per-user tracking seems to have an issue."
        );
    } else {
        println!("  No user IDs were mapped and changed in the TSV based on the provided map.");
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli_args = CliArgs::parse();
    println!("Starting Jellyfin TSV updater.");
    println!(
        "Attempting to load configuration from: {}",
        cli_args.config_file_path
    );

    // Load configuration
    let mut config = match load_config(&cli_args.config_file_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!(
                "Failed to load configuration using '{}' or fallback 'config.example.toml': {}",
                cli_args.config_file_path, e
            );
            // Create a default config or panic, depending on desired behavior
            // For now, let's use a placeholder that would cause issues, to highlight the problem
            // In a real app, you'd handle this more gracefully.
            return Err(Box::new(e) as Box<dyn Error>);
        }
    };

    // Normalize base_url for instance_old
    if !config.instance_old.base_url.contains("://") {
        config.instance_old.base_url = format!("http://{}", config.instance_old.base_url);
    }
    if config.instance_old.base_url.ends_with('/') {
        config.instance_old.base_url.pop();
    }

    // Normalize base_url for instance_new
    if !config.instance_new.base_url.contains("://") {
        config.instance_new.base_url = format!("http://{}", config.instance_new.base_url);
    }
    if config.instance_new.base_url.ends_with('/') {
        config.instance_new.base_url.pop();
    }

    println!("Configuration loaded (and URLs normalized): {:?}", config);

    let client = Client::new();
    let mut old_users_vec: Vec<JellyfinUser> = Vec::new();
    let mut new_users_vec: Vec<JellyfinUser> = Vec::new();

    // Fetch users from old instance
    println!("\nFetching users from OLD instance...");
    match fetch_users_from_instance(&config.instance_old, &client).await {
        Ok(users) => {
            println!(
                "Successfully fetched {} users from old instance.",
                users.len()
            );
            for user in users.iter().take(3) {
                // Print first 3 users as sample
                println!("  User: Name='{}', ID='{}'", user.name, user.id);
            }
            old_users_vec = users; // Store fetched users
        }
        Err(e) => {
            eprintln!("Error fetching users from old instance: {}", e);
        }
    }

    // Fetch users from new instance
    println!("\nFetching users from NEW instance...");
    match fetch_users_from_instance(&config.instance_new, &client).await {
        Ok(users) => {
            println!(
                "Successfully fetched {} users from new instance.",
                users.len()
            );
            for user in users.iter().take(3) {
                // Print first 3 users as sample
                println!("  User: Name='{}', ID='{}'", user.name, user.id);
            }
            new_users_vec = users; // Store fetched users
        }
        Err(e) => {
            eprintln!("Error fetching users from new instance: {}", e);
        }
    }

    if old_users_vec.is_empty() && new_users_vec.is_empty() {
        // Corrected logic: if BOTH are empty, it's problematic for mapping.
        println!("Both user lists are empty. Cannot create a meaningful user map. TSV processing will likely do nothing or copy the file.");
        // Allow to proceed, create_user_id_map will return an empty map, and process_tsv_file handles an empty map.
    } else if old_users_vec.is_empty() {
        println!("Old user list is empty. No users to map from. TSV processing will likely do nothing or copy the file.");
    } else if new_users_vec.is_empty() {
        println!("New user list is empty. No users to map to. TSV processing will likely do nothing or copy the file.");
    }

    // These lines call the functions:
    let user_id_map = create_user_id_map(&old_users_vec, &new_users_vec);
    process_tsv_file(&config, &user_id_map).await?;

    println!("\nJellyfin TSV updater finished successfully.");
    Ok(())
}
