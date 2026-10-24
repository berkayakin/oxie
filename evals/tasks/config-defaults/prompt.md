A service reads its settings from text like this (put it in the program as a constant):

```
# server settings
host = example.org
port = 8080

timeout =
retries = three
```

Each setting is a `key = value` line; blank lines and lines starting with `#` are ignored.
Parse the text into a data class with nullable fields `host`, `port`, `timeout`, `retries` and
`user`: a setting that is missing, empty, or not a valid number is null. Then print the
settings, using defaults for the missing ones (host "localhost", port 80, timeout 30,
retries 3, user "(none)"), and the length of the host name (0 if there is none). Use Oxie's
null-safety operators rather than unwrapping.

The program must print exactly:

```
host: example.org
port: 8080
timeout: 30 (default)
retries: 3 (default)
user: (none)
host length: 11
```
