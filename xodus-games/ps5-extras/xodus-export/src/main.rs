// Copies the Xodus sign-in (four Keychain entries under "Xodus Service") into
// a keyring-core sample store file, the format xodus reads with its
// key-chain-file feature. Usage: xodus-export <out.ron>
const SERVICE: &str = "Xodus Service";
const KEYS: [&str; 4] = ["dev_license", "device-tokens", "user-tokens", "user-DA"];

fn main() {
    let out = std::env::args().nth(1).expect("usage: xodus-export <out.ron>");
    keyring_core::set_default_store(apple_native_keyring_store::keychain::Store::new().unwrap());
    let mut secrets = Vec::new();
    for key in KEYS {
        match keyring_core::Entry::new(SERVICE, key).and_then(|e| e.get_secret()) {
            Ok(secret) => {
                println!("{key}: {} bytes", secret.len());
                secrets.push((key, secret));
            }
            Err(err) => println!("{key}: {err}"),
        }
    }
    keyring_core::unset_default_store();
    let file = keyring_core::sample::Store::new_with_backing(&out).unwrap();
    keyring_core::set_default_store(file.clone());
    for (key, secret) in &secrets {
        keyring_core::Entry::new(SERVICE, key).unwrap().set_secret(secret).unwrap();
    }
    file.save().unwrap();
    keyring_core::unset_default_store();
    println!("wrote {} entries to {out}", secrets.len());
}
