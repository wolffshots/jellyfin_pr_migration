# Jellyfin PlaybackReporting Migration Tool (jellyfin_pr_migration)

This tool facilitates the migration of playback history (specifically from the Jellyfin PlaybackReporting plugin's data format) from one Jellyfin instance to another. It works by:
1. Fetching user lists from both the old and new Jellyfin instances.
2. Mapping users based on identical usernames to find corresponding old and new User IDs.
3. Processing an input TSV file (expected to be an export from the old instance's PlaybackReporting plugin, typically header-less).
4. Replacing the old User IDs in the playback records with the new User IDs.
5. Outputting the modified records, either to a new TSV file (header-less) and/or by inserting them directly into an SQLite database table (e.g., `playback_reporting.db` used by the PlaybackReporting plugin on the new instance).

Running the tool multiple times will not cause any issues, as it will only update the records that have not been processed yet.

Note: for best practice you should probably make a copy of the `playback_reporting.db` file and use that to actually work on. Once you are happy with the updated database locally you should shutdown the new Jellyfin instance and replace the original `playback_reporting.db` with the updated one.

## Features

*   Connects to two Jellyfin instances via their APIs using API tokens.
*   Fetches user lists (`Name` and `Id`) from both instances.
*   Creates a mapping from old user IDs to new user IDs for users found in both instances (matched by `Name`).
*   Reads an input TSV file (assumed to be header-less).
*   Replaces `UserId` values in the TSV data based on the generated mapping.
*   Optionally writes the modified data to an output TSV file (header-less).
*   Optionally inserts the modified data into a specified table in an SQLite database.
    *   Includes transaction support for efficient bulk inserts.
    *   Performs a check to avoid inserting duplicate records if they already exist in the database table.
*   Provides a summary of changes, including User ID mappings, records processed, records changed, records inserted into SQLite, and records skipped as duplicates.
*   Configuration via a `config.toml` file (supports custom path via CLI argument).
*   Handles basic URL normalization for Jellyfin instance base URLs.
*   Displays a live progress bar during TSV/DB processing.

## Configuration (`config.toml`)

Copy `config.example.toml` to `config.toml` in the same directory as the executable, or provide a path to your config file using the `-c` argument.

Update the `config.toml` with your details:

```toml
# Path to the input TSV file (header-less) from the old Jellyfin instance's
# PlaybackReporting plugin data.
input_tsv_file_path = "path/to/your/input.tsv"

# --- Output Options ---
# You can enable TSV output, SQLite output, or both.
# If neither is configured, the tool will process data but not save it anywhere.

# Option 1: Output to TSV file (header-less)
# If not needed, comment out or remove this line.
output_tsv_file_path = "path/to/your/output.tsv"

# Option 2: Output to SQLite database
# If enabled, data will be inserted into the specified table.
# The SQLite database file and the target table are assumed to already exist.
# The table should have columns matching the TSV structure (e.g., DateCreated, UserId, etc.),
# typically all TEXT type for compatibility with PlaybackReporting plugin's schema.
#
# To enable, uncomment and set the following:
# sqlite_db_path = "path/to/your/playback_reporting.db"
# sqlite_table_name = "PlaybackActivity" # Defaults to "PlaybackActivity" if not specified

[instance_old]
base_url = "http://your-old-jellyfin-url.com" # Or just "your-old-jellyfin-url.com:8096"
api_token = "YOUR_OLD_JELLYFIN_API_TOKEN"

[instance_new]
base_url = "http://your-new-jellyfin-url.com" # Or just "your-new-jellyfin-url.com:8096"
api_token = "YOUR_NEW_JELLYFIN_API_TOKEN"
```

## Usage

### Prebuilt release

Download the latest release from [GitHub Releases](https://github.com/wolffshots/jellyfin-pr-migration/releases).

```bash
./jellyfin_pr_migration
```
or
```bash
./jellyfin_pr_migration -c /path/to/your/custom_config.toml
```

### Building from source

```bash
# Build the project
cargo build
```

Run it with
```bash
./target/debug/jellyfin_pr_migration
```
or
```bash
./target/debug/jellyfin_pr_migration -c /path/to/your/custom_config.toml
```

### Using Docker

A Docker image is available on GitHub Container Registry. This simplifies deployment and eliminates the need to install Rust or build the application locally.

#### Prepare your data directory

Create a directory on your host machine to store your configuration and data files:

```bash
mkdir -p /path/to/your/data
```

Place your `config.toml`, `input.tsv`, and optionally your `playback_reporting.db` in this directory. Make sure to update your `config.toml` with paths relative to the Docker container's `/data` directory:

```toml
# In your config.toml, use paths relative to /data
input_tsv_file_path = "/data/input.tsv"
output_tsv_file_path = "/data/output.tsv" # optional if sqlite_db_path is set - see config.example.toml or Configuration in README.md more context
# sqlite_db_path = "/data/playback_reporting.db"
# sqlite_table_name = "PlaybackActivity" # Defaults to "PlaybackActivity" if not specified

[instance_old]
base_url = "http://localhost:8096"
api_token = "YOUR_OLD_JELLYFIN_API_TOKEN"

[instance_new]
base_url = "http://localhost:8097"
api_token = "YOUR_NEW_JELLYFIN_API_TOKEN"
```

#### Run the Docker container

The image is amd64 so will require you to be able to run amd64 containers.

```bash
docker run -it --rm -v /path/to/your/data:/data ghcr.io/wolffshots/jellyfin_pr_migration:latest
```

If you are on a non-amd64 arch then you may need to specify platform as well:
```bash
docker run -it --rm -v /path/to/your/data:/data --platform=linux/amd64 ghcr.io/wolffshots/jellyfin_pr_migration:latest
```

If you run into permission issues then you may need to chown the directory and try run it again:
```bash
sudo chown -R 144:153 /path/to/your/data
```

The container will:
1. Mount your host directory to `/data` inside the container
2. Use the `config.toml` from this directory
3. Process your input files and create output files in the same directory or update the SQLite3 database with the relevant data
4. Exit when processing is complete

This allows you to run the migration tool without installing any dependencies on your host system.

## TODO

*   [ ] **Automatic HTTP to HTTPS Upgrade**: Implement logic to attempt connection via HTTPS if an HTTP connection to a Jellyfin instance fails or is redirected.
*   [ ] **More Robust Error Handling**: Enhance error handling for API interactions and file operations.
*   [ ] **Testing**: Add unit and integration tests.
*   [ ] **Logging Levels**: Implement configurable logging levels (e.g., debug, info, error).
*   [x] **Docker Support**: Add support for running the migration tool within a Docker container.
