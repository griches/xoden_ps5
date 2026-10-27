//! Explicit certificate bundle for an embedded host without a system trust store.
//! Desktop callers retain their existing platform verifier unless configured.
use std::sync::OnceLock;
static TRUST: OnceLock<(Vec<u8>, Vec<reqwest::Certificate>)> = OnceLock::new();

fn parse(pem: &[u8]) -> Result<Vec<reqwest::Certificate>, &'static str> {
    if pem.is_empty() || pem.len() > 2 * 1024 * 1024 { return Err("trust-size"); }
    let certificates = reqwest::Certificate::from_pem_bundle(pem).map_err(|_| "trust-pem")?;
    if certificates.is_empty() || certificates.len() > 512 { return Err("trust-count"); }
    reqwest::Client::builder().tls_certs_only(certificates.clone()).build().map_err(|_| "trust-certificate")?;
    Ok(certificates)
}
/// Configure once before creating job clients; repeated identical configuration
/// is allowed. A live process cannot silently switch its trust anchors.
pub fn configure(pem: &[u8]) -> Result<(), &'static str> {
    if let Some((existing, _)) = TRUST.get() {
        return if existing == pem { Ok(()) } else { Err("trust-already-configured") };
    }
    let certificates = parse(pem)?;
    if TRUST.set((pem.to_vec(),certificates)).is_err() && TRUST.get().is_none_or(|(existing,_)| existing != pem) {
        return Err("trust-already-configured");
    }
    Ok(())
}
pub fn builder() -> Result<reqwest::ClientBuilder, &'static str> {
    let builder = reqwest::Client::builder();
    #[cfg(all(target_os = "freebsd", feature = "ps5-native-platform"))]
    let builder = builder.dns_resolver(std::sync::Arc::new(ps5_dns::Resolver));
    match TRUST.get() {
        Some((_, certificates)) => Ok(builder.tls_certs_only(certificates.clone())),
        None => {
            #[cfg(target_os = "freebsd")]
            return Err("trust-required");
            #[cfg(not(target_os = "freebsd"))]
            Ok(builder)
        }
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn rejects_empty_malformed_and_oversized_bundles() {
        for bytes in [Vec::new(), b"not a certificate".to_vec(), vec![0;2*1024*1024+1], b"-----BEGIN CERTIFICATE-----\ninvalid\n-----END CERTIFICATE-----".to_vec()] {
            assert!(super::parse(&bytes).is_err());
        }
    }
}

// The PS5 title has no libc getaddrinfo export. Keep the original hostname in
// the request (including TLS verification/SNI); replace only DNS resolution.
#[cfg(all(target_os = "freebsd", feature = "ps5-native-platform"))]
mod ps5_dns {
    use reqwest::dns::{Addrs, Name, Resolve, Resolving};
    use std::{ffi::CString, io, net::{Ipv4Addr, SocketAddr}};
    unsafe extern "C" {
        fn xodus_ps5_resolve_ipv4(host: *const std::ffi::c_char, address: *mut u8) -> i32;
    }
    pub struct Resolver;
    impl Resolve for Resolver {
        fn resolve(&self, name: Name) -> Resolving {
            let host = CString::new(name.as_str());
            Box::pin(async move {
                let host = host?;
                // Console DNS is synchronous and bounded to five seconds. The
                // job drains this blocking task before cancellation completes.
                let address = tokio::task::spawn_blocking(move || {
                    let mut octets = [0u8; 4];
                    let code = unsafe { xodus_ps5_resolve_ipv4(host.as_ptr(), octets.as_mut_ptr()) };
                    if code != 0 { return Err(io::Error::other("console DNS lookup failed")); }
                    Ok(SocketAddr::from((Ipv4Addr::from(octets), 0)))
                }).await??;
                Ok(Box::new(std::iter::once(address)) as Addrs)
            })
        }
    }
}
