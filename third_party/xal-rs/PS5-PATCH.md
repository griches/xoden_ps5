Vendored from https://github.com/xodus-gaming/xal-rs at 6aa2420f8f0d5e6f7460ff13f399037583a942f4.

The MIT license and original source/tests are retained. The only local change
is target-specific reqwest TLS selection: Rustls for FreeBSD/PS5, original
default TLS for every other target. This avoids an unavailable OpenSSL runtime
in the native PS5 build. No authentication or entitlement behaviour is changed.
