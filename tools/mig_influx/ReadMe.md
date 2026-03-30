Small tool that migrates data from an old InfluxDB instance to a new one, or to another database or table, in chunks of time.
It retrieves the oldest time for each table, then migrates data in chunks until it reaches the current time.
It can rename tables, columns or map values during the migration.

