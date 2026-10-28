# Credential record formatting

The generated C# record formatting for `RegisterRequest`, `LoginRequest` and
`SessionView` included password or access-token values. Each type now overrides
`ToString()` with a static redacted description, including on record copies.
This closes incidental string-formatting exposure without changing wire fields,
password verification, token handling, endpoints or database contents.

The added regression checks exercise the actual backend types through object
formatting, interpolation and composite formatting, then verify that explicit
JSON requests and responses retain their existing fields. Synthetic values are
created only in memory; reports contain static check names and counts.

This is not general-purpose credential erasure or a promise that arbitrary
serializers redact credentials. JSON serialization intentionally retains values
needed for the existing loopback authentication API. Callers must not log those
serialized request or response bodies.

Locked Release builds passed with zero warnings or errors. The parent and an
independent reviewer each ran the actual backend test executable: 101 checks,
including nine formatting/wire checks and 83 isolated HTTP requests. A separate
disposable baseline compiled the original record definitions with the same new
checks and failed the first formatting assertion as expected. Neither the
baseline failure nor the passing reports print credential values. Existing
installed services, original client files and real account data were not used.
