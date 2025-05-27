## Testing setup

1. Start the Docker compose stack

2. Add a user to each of the instances (localhost:8097 and localhost:8098 in your browser respectively)

3. Go to the profile of the user on Jellyfin1 and get their ID from the url

4. Run `./generate.sh jellyfin1.db <their id> 10` to generate 10 randomised records in a new test origin database

5. Export those records to a test input TSV with: `sqlite3 -separator $'\t' jellyfin1.db "SELECT * FROM PlaybackActivity;" > input.tsv`

6. Go to the profile of the user on Jellyfin2 and get their ID from the url

7. Run `./generate.sh jellyfin2.db <their id> 10` to generate 10 randomised records in a new test destination database

8. Do your testing using the 2 Jellyfin instances and the randomised play data in the databases (with input.tsv as the input and either creating an output.tsv or editing jellyfin2.db - the output.tsv will be easiest though)

9. Attach logging output and the used input and output TSVs (all in code blocks) or databases to PRs as testing evidence

