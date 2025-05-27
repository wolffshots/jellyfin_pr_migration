#!/bin/bash

if [ $# -lt 2 ]; then
    echo "Error: User ID is required"
    echo "Usage: $0 <database_path> <user_id> [num_records]"
    exit 1
fi

DB_PATH=$1
USER_ID=$2
NUM_RECORDS=${3:-50}

# Create tables if they don't exist
sqlite3 "$DB_PATH" "
CREATE TABLE IF NOT EXISTS PlaybackActivity (
    DateCreated DATETIME NOT NULL,
    UserId TEXT,
    ItemId TEXT,
    ItemType TEXT,
    ItemName TEXT,
    PlaybackMethod TEXT,
    ClientName TEXT,
    DeviceName TEXT,
    PlayDuration INT
);

CREATE TABLE IF NOT EXISTS UserList (
    UserId TEXT
);"

# Random data arrays
ITEM_TYPES=("Movie" "Episode" "Audio" "MusicVideo" "AudioBook")
CLIENTS=("Jellyfin Web" "Jellyfin Android" "Jellyfin iOS" "Infuse" "VLC")
DEVICES=("Chrome Browser" "Samsung Galaxy" "iPhone 13" "Apple TV" "Windows PC")
METHODS=("DirectPlay" "Transcode" "DirectStream")
ITEMS=("The Matrix" "Breaking Bad S01E01" "Inception" "Stranger Things S04E09" "Dune" "The Office S02E03")

generate_record() {
    local date_created=$(date -d "$((RANDOM % 365)) days ago" '+%Y-%m-%d %H:%M:%S')
    local item_type=${ITEM_TYPES[$((RANDOM % ${#ITEM_TYPES[@]}))]}
    local item_name=${ITEMS[$((RANDOM % ${#ITEMS[@]}))]}
    local client=${CLIENTS[$((RANDOM % ${#CLIENTS[@]}))]}
    local device=${DEVICES[$((RANDOM % ${#DEVICES[@]}))]}
    local method=${METHODS[$((RANDOM % ${#METHODS[@]}))]}
    local duration=$((RANDOM % 7200 + 300))
    local item_id="item_$((RANDOM % 1000))"
    
    echo "INSERT INTO PlaybackActivity VALUES ('$date_created', '$USER_ID', '$item_id', '$item_type', '$item_name', '$method', '$client', '$device', $duration);"
}

# Insert user and generate records
echo "INSERT OR IGNORE INTO UserList VALUES ('$USER_ID');" | sqlite3 "$DB_PATH"

for ((i=1; i<=NUM_RECORDS; i++)); do
    generate_record | sqlite3 "$DB_PATH"
done

echo "Generated $NUM_RECORDS records for user: $USER_ID in $DB_PATH"
