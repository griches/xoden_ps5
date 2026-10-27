//! Private, bounded Playden job protocol. Requests contain identities, never paths,
//! credentials or CDN URLs. A prepared result is not a published Playden install.
use std::{collections::{BTreeMap, HashSet}, fs::{File, OpenOptions}, io::{Read, Write, Seek, SeekFrom}, path::Path, process::ExitCode, time::Duration};
use fs2::FileExt;
use serde_json::{Value, json};
use xodus::tokens::TokenManager;

const PLANS: &[&str] = &["CFQ7TTC0KGQ8", "CFQ7TTC0KHS0", "CFQ7TTC0P85B", "CFQ7TTC0K5DJ", "CFQ7TTC0K6L8", "CFQ7TTC10QFD"];
const PC_PLANS: &[&str] = &["CFQ7TTC0KGQ8", "CFQ7TTC0KHS0"];
#[path = "ps5_signin.rs"]
mod signin;
type Result<T> = std::result::Result<T, &'static str>;

struct Request { id: String, operation: String, market: String, account: Option<String>, product: Option<String>, content: Option<String>, observer: Option<crate::job_observer::Observer> }
fn product_id(s: &str) -> bool { s.len() == 12 && s.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) }
fn hash(s: &str) -> bool { s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
fn parse(bytes: &[u8]) -> Result<Request> {
    if bytes.len() > 4096 { return Err("request-size"); }
    let v: Value = serde_json::from_slice(bytes).map_err(|_| "request-json")?;
    let obj = v.as_object().ok_or("request-shape")?;
    if v["schema"] != 1 || obj.keys().any(|k| !["schema","requestId","operation","market","accountKey","productId","contentId"].contains(&k.as_str())) { return Err("request-schema"); }
    let id = v["requestId"].as_str().ok_or("request-id")?;
    if uuid::Uuid::parse_str(id).map(|u| u.to_string()).ok().as_deref() != Some(id) { return Err("request-id"); }
    let operation = v["operation"].as_str().filter(|s| ["refresh","plan","install","signin-test","signout"].contains(s)).ok_or("request-operation")?;
    let market = v["market"].as_str().filter(|s| s.len() == 2 && s.bytes().all(|b| b.is_ascii_uppercase())).ok_or("request-market")?;
    let optional = |key: &str| -> Result<Option<String>> { match v.get(key) { None | Some(Value::Null) => Ok(None), Some(Value::String(s)) => Ok(Some(s.clone())), _ => Err("request-field") } };
    let account = optional("accountKey")?;
    let product = optional("productId")?;
    let content = optional("contentId")?;
    if account.as_deref().is_some_and(|s| !hash(s)) || product.as_deref().is_some_and(|s| !product_id(s))
        || content.as_deref().is_some_and(|s| uuid::Uuid::parse_str(s).map(|u| u.to_string()).ok().as_deref() != Some(s)) { return Err("request-identity"); }
    if (operation == "install" || operation == "plan") && (account.is_none() || product.is_none()) { return Err("request-install-identity"); }
    if operation == "refresh" && (product.is_some() || content.is_some()) { return Err("request-refresh-fields"); }
    if operation == "signin-test" && (product.is_none() || account.is_some()) { return Err("request-signin-fields"); }
    if operation == "signout" && (product.is_some() || content.is_some() || account.is_some()) { return Err("request-signout-fields"); }
    Ok(Request { id: id.into(), operation: operation.into(), market: market.into(), account, product, content, observer: None })
}

fn read(path: &Path, max: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path).map_err(|_| "file-open")?.take(max + 1).read_to_end(&mut bytes).map_err(|_| "file-read")?;
    if bytes.len() as u64 > max { return Err("file-size"); }
    Ok(bytes)
}
fn publish(root: &Path, name: &str, value: &Value) -> Result<()> {
    let bytes = serde_json::to_vec(value).map_err(|_| "result-json")?;
    if bytes.len() > 16 * 1024 * 1024 { return Err("result-size"); }
    let temp = root.join(format!(".{name}.tmp"));
    let mut options = OpenOptions::new(); options.create(true).truncate(true).write(true);
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
    let mut file = options.open(&temp).map_err(|_| "result-open")?;
    file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(|_| "result-write")?;
    drop(file);
    super::durable_file::publish(&temp, &root.join(name)).map_err(|_| "result-publish")
}
fn state(root: &Path, request: &Request, status: &str, detail: Value) -> Result<()> {
    #[cfg(windows)]
    crate::ps5_worker::phase(status);
    publish(root, "job-result.json", &json!({"schema":1,"requestId":request.id,"operation":request.operation,
        "productId":request.product,"status":status,"updatedAt":chrono::Utc::now().to_rfc3339(),"detail":detail}))?;
    if let Some(observer) = &request.observer { observer.phase(status); }
    Ok(())
}

// Apply the same provenance rules as Nexus, rejecting unresolved parents rather
// than promoting a library classification into permission to download.
fn entitled(product: &str, rows: &BTreeMap<String, Value>, now: i64) -> bool {
    let mut current = product;
    let mut visited = HashSet::new();
    for depth in 0..16 {
        if !visited.insert(current) { return false; }
        let Some(row) = rows.get(current) else { return depth > 0 && PLANS.contains(&current); };
        if row["status"] != "Active" || row["isTrial"] != false || row["sharingSource"] != "None"
            || (row["special"] != false && row["isPositron"] != true)
            || row["endDate"].as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).is_none_or(|t| t.timestamp() <= now) { return false; }
        match row["satisfiedByProductId"].as_str() {
            Some(parent) => current = parent,
            None => return !PLANS.contains(&product),
        }
    }
    false
}

fn text(value: &Value, max: usize) -> Option<&str> { value.as_str().filter(|s| !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control)) }
fn catalog_rows(body: &Value, requested: &[String]) -> Result<Vec<Value>> {
    let products = body["Products"].as_array().filter(|p| p.len() <= 60).ok_or("catalog-shape")?;
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    for p in products {
        let id = p["ProductId"].as_str().filter(|id| requested.iter().any(|r| r == id)).ok_or("catalog-identity")?;
        if !seen.insert(id) { return Err("catalog-duplicate"); }
        if p["ProductType"] != "Game" { continue; }
        let localized = &p["LocalizedProperties"][0];
        let title = text(&localized["ProductTitle"], 512).ok_or("catalog-title")?;
        let image = localized["Images"].as_array().and_then(|images| images.iter().find_map(|image| {
            if !["BoxArt","Poster"].contains(&image["ImagePurpose"].as_str()?) { return None; }
            let raw = image["Uri"].as_str()?;
            let raw = if raw.starts_with("//") { format!("https:{raw}") } else { raw.into() };
            let url = reqwest::Url::parse(&raw).ok()?;
            (url.scheme() == "https" && url.host_str() == Some("store-images.s-microsoft.com") && url.username().is_empty()
                && url.password().is_none() && url.port().is_none() && raw.len() < 4096).then_some(raw)
        }));
        rows.push(json!({"productId":id,"title":title,"developer":text(&localized["DeveloperName"],512),
            "description":localized["ShortDescription"].as_str().filter(|s|s.len()<=8192),"artwork":image}));
    }
    Ok(rows)
}

fn program_path(value: &str) -> Result<String> {
    let value = value.replace('\\', "/");
    if value.is_empty() || value.len() > 256 || !value.bytes().all(|b| (32..127).contains(&b))
        || value.contains(':') || value.split('/').any(|p| p.is_empty() || p == "." || p == "..")
        || !value.to_ascii_lowercase().ends_with(".exe") { return Err("program-path"); }
    Ok(value)
}
fn declared_program(bytes: &[u8]) -> Result<String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_reader(bytes);
    let mut stack: Vec<String> = Vec::new();
    let mut programs = HashSet::new();
    loop {
        let event = reader.read_event().map_err(|_| "game-config-xml")?;
        let starts_element = matches!(&event, Event::Start(_));
        match event {
            Event::Start(ref element) | Event::Empty(ref element) => {
                if stack.len() > 16 { return Err("game-config-depth"); }
                let name = element.local_name();
                if name.as_ref() == "Executable" && stack.iter().map(String::as_str).eq(["Game", "ExecutableList"]) {
                    let mut program = None; let mut eligible = true;
                    for attribute in element.attributes() {
                        let attribute = attribute.map_err(|_| "game-config-attribute")?;
                        let value = attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0).map_err(|_| "game-config-attribute")?;
                        match attribute.key.as_ref() {
                            "Name" => program = Some(program_path(&value)?),
                            "IsDevOnly" if value != "false" => eligible = false,
                            "TargetDeviceFamily" if value != "PC" => eligible = false,
                            _ => (),
                        }
                    }
                    if eligible { programs.insert(program.ok_or("game-config-program")?); }
                }
                if starts_element { stack.push(name.as_ref().to_owned()); }
            }
            Event::End(_) => { if stack.pop().is_none() { return Err("game-config-xml"); } }
            Event::DocType(_) => return Err("game-config-doctype"),
            Event::Eof => break,
            _ => (),
        }
    }
    if !stack.is_empty() || programs.len() != 1 { return Err("program-choice-required"); }
    programs.into_iter().next().ok_or("program-choice-required")
}
fn executable(root: &Path) -> Result<String> {
    let program = declared_program(&read(&root.join("MicrosoftGame.config"), 1024 * 1024)?)?;
    let mut path = root.to_owned();
    for part in program.split('/') {
        path.push(part);
        if std::fs::symlink_metadata(&path).map_err(|_| "program-missing")?.file_type().is_symlink() { return Err("program-symlink"); }
    }
    let mut file = File::open(path).map_err(|_| "program-open")?;
    let mut header = [0u8;64]; file.read_exact(&mut header).map_err(|_| "program-header")?;
    if &header[..2] != b"MZ" { return Err("program-header"); }
    let offset = u32::from_le_bytes(header[60..64].try_into().map_err(|_| "program-header")?);
    if !(64..=1024*1024).contains(&offset) { return Err("program-header"); }
    file.seek(SeekFrom::Start(offset.into())).map_err(|_| "program-header")?;
    let mut pe = [0u8;6]; file.read_exact(&mut pe).map_err(|_| "program-header")?;
    if pe != *b"PE\0\0\x64\x86" { return Err("program-x64-required"); }
    Ok(program)
}
fn remember_prepared(root: &Path, entry: Value) -> Result<()> {
    let path = root.join("prepared-games.json");
    let mut rows = if path.exists() {
        let value: Value = serde_json::from_slice(&read(&path, 2*1024*1024)?).map_err(|_| "prepared-index-json")?;
        if value["schema"] != 1 { return Err("prepared-index-schema"); }
        value["games"].as_array().filter(|r| r.len() <= 400).ok_or("prepared-index-shape")?.clone()
    } else { Vec::new() };
    rows.retain(|row| row["productId"] != entry["productId"]);
    if rows.len() >= 400 { return Err("prepared-index-full"); }
    rows.push(entry);
    publish(root, "prepared-games.json", &json!({"schema":1,"games":rows}))
}

async fn execute(client: &reqwest::Client, tokens: &TokenManager, root: &Path, games: &Path, req: &Request) -> Result<()> {
    if req.operation == "signout" {
        state(root,req,"signing-out",json!({}))?;
        tokens.remove_persistent().map_err(|_| "signout-credentials")?;
        match std::fs::remove_file(root.join("catalog.json")) {
            Ok(())=>(),Err(error) if error.kind()==std::io::ErrorKind::NotFound=>(),Err(_)=>return Err("signout-catalog"),
        }
        return state(root,req,"signed-out",json!({"accountCleared":true}));
    }
    if req.operation == "signin-test" { return signin::run(client,tokens,root,req).await; }
    state(root, req, "checking-account", json!({}))?;
    let account = tokens.get_user().map_err(|_| "sign-in-required")?.username;
    let account_key = msixvc::streaming::resume_digest(account.as_bytes());
    if req.account.as_ref().is_some_and(|key| key != &account_key) { return Err("account-changed"); }
    let (membership, library) = tokio::try_join!(
        super::subscription::query(tokens, &account), super::library::query(tokens, &req.market))?;
    if library["account"] != account { return Err("account-changed"); }
    let subscription = membership.0;
    let active = subscription["status"] == "active" && subscription["productIds"].as_array().is_some_and(|ids|
        ids.iter().any(|id| id.as_str().is_some_and(|s| PC_PLANS.contains(&s))));
    let entries: BTreeMap<String, Value> = library["entitlements"].as_array().ok_or("library-shape")?.iter()
        .filter_map(|v| v["productId"].as_str().map(|id| (id.into(), v.clone()))).collect();
    if req.operation == "refresh" {
        let profile = super::profile::profile(tokens).await?;
        if profile["account"] != account { return Err("account-changed"); }
        let ids: Vec<_> = entries.keys().filter(|id| entitled(id, &entries, chrono::Utc::now().timestamp())).cloned().collect();
        let public = crate::transport::builder()?.redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(30)).build().map_err(|_| "catalog-client")?;
        let mut rows = Vec::new();
        for chunk in ids.chunks(60) {
            state(root, req, "refreshing", json!({"completed":rows.len(),"totalEntitlements":ids.len()}))?;
            let mut url = reqwest::Url::parse("https://displaycatalog.mp.microsoft.com/v7.0/products").map_err(|_| "catalog-url")?;
            url.query_pairs_mut().append_pair("bigIds", &chunk.join(",")).append_pair("market", &req.market).append_pair("languages", "en-us,neutral");
            let response = public.get(url)
                .send().await.map_err(|_| "catalog-request")?;
            let bytes = super::profile::bounded(response, 8 * 1024 * 1024).await?;
            rows.extend(catalog_rows(&serde_json::from_slice(&bytes).map_err(|_| "catalog-json")?, chunk)?);
        }
        if tokens.get_user().map_err(|_| "account-changed")?.username != account { return Err("account-changed"); }
        let count = rows.len();
        publish(root, "catalog.json", &json!({"schema":1,"requestId":req.id,"accountKey":account_key,"gamertag":profile["gamertag"],
            "market":req.market,"subscriptionActive":active,"checkedAt":library["checkedAt"],"games":rows,
            "skippedEntitlements":library["skippedEntitlements"]}))?;
        return state(root, req, "complete", json!({"accountKey":account_key,"games":count}));
    }
    if !active { return Err("active-pc-subscription-required"); }
    let product = req.product.as_deref().ok_or("request-product")?;
    if !entitled(product, &entries, chrono::Utc::now().timestamp()) { return Err("game-entitlement-required"); }
    state(root, req, "resolving", json!({}))?;
    let plan = crate::package_plan::resolve(client, product, &req.market).await.map_err(|_| "package-resolution")?.without_durable_selection();
    if req.operation == "plan" { return state(root, req, "package-choice", plan.json(product, &req.market)); }
    let content = match plan.select(req.content.as_deref()) {
        Ok(content) => content,
        Err(_) => return state(root, req, "package-choice", plan.json(product, &req.market)),
    };
    let folder = format!("xbox-{}", product.to_ascii_lowercase());
    let destination = games.join(&folder);
    std::fs::create_dir_all(&destination).map_err(|_| "download-directory")?;
    state(root, req, "downloading", json!({"contentId":content}))?;
    if super::streaming::run_observed(client,tokens,product.into(),destination.to_string_lossy().into(),false,None,Some(req.market.clone()),true,false,Some(content.clone()),req.observer.clone()).await != ExitCode::SUCCESS { return Err(req.observer.as_ref().and_then(|observer| observer.failure_code()).unwrap_or("download-failed")); }
    state(root, req, "preparing", json!({"contentId":content}))?;
    if super::prepare_ps5::run(client,tokens,destination.to_string_lossy().into(),Some(req.market.clone())).await != ExitCode::SUCCESS { return Err("preparation-failed"); }
    if tokens.get_user().map_err(|_| "account-changed")?.username != account { return Err("account-changed"); }
    let program = executable(&destination)?;
    let entry = json!({"accountKey":account_key,"folder":folder,"contentId":content,"executable":program,
        "requestId":req.id,"productId":product});
    remember_prepared(root, entry.clone())?;
    state(root, req, "prepared", entry)
}

pub async fn run(client: &reqwest::Client, tokens: &TokenManager, root: &Path, games: &Path) -> ExitCode {
    run_observed(client,tokens,root,games,None).await
}

/// Owns the job lock independently of the async operation. Embedded hosts must
/// retain this lease until their dedicated runtime has drained blocking file IO.
/// Dropping only the operation future does not make partial files safe to delete.
pub struct JobLease {
    _lock: File,
    root: std::path::PathBuf,
    games: std::path::PathBuf,
    request: Request,
}
impl JobLease {
    pub fn open(root: &Path, games: &Path, observer: Option<crate::job_observer::Observer>) -> Result<Self> {
        let lock = OpenOptions::new().create(true).truncate(false).write(true)
            .open(root.join("job.lock")).map_err(|_| "job-lock-open")?;
        lock.try_lock_exclusive().map_err(|_| "job-busy")?;
        let mut request = parse(&read(&root.join("job-request.json"),4096)?)?;
        request.observer = observer;
        Ok(Self { _lock: lock, root: root.into(), games: games.into(), request })
    }
    pub async fn run(&self, client: &reqwest::Client, tokens: &TokenManager) -> ExitCode {
        match execute(client,tokens,&self.root,&self.games,&self.request).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(stage) => {
                // Only static stage identifiers enter the host mailbox. Never
                // forward an upstream error body, URL or credential here.
                if let Some(observer) = &self.request.observer { observer.diagnostic(stage); }
                let _ = state(&self.root,&self.request,"failed",json!({"code":stage}));
                eprintln!("PS5_JOB_ERROR {stage}");
                ExitCode::FAILURE
            }
        }
    }
    /// Call only after the dedicated runtime has stopped and drained all IO.
    /// This records an interruption; it never deletes a partial download.
    pub fn record_cancelled_after_drain(&self) -> Result<()> {
        state(&self.root,&self.request,"cancelled",json!({"code":"user-cancelled"}))
    }
}

pub async fn run_observed(client: &reqwest::Client, tokens: &TokenManager, root: &Path, games: &Path, observer: Option<crate::job_observer::Observer>) -> ExitCode {
    // Never create a fallback directory or use a request-supplied path.
    match JobLease::open(root,games,observer) {
        Ok(job) => job.run(client,tokens).await,
        Err(stage) => { eprintln!("PS5_JOB_ERROR {stage}"); ExitCode::FAILURE }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn native_failure_reports_only_static_stage_to_host_observer() {
        struct Capture(std::sync::Mutex<Option<&'static str>>);
        impl crate::job_observer::JobObserver for Capture {
            fn diagnostic(&self, code: &'static str) { *self.0.lock().unwrap() = Some(code); }
        }
        let root = tempfile::tempdir().unwrap();
        let games = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("job-request.json"), serde_json::to_vec(&request()).unwrap()).unwrap();
        let capture = std::sync::Arc::new(Capture(std::sync::Mutex::new(None)));
        let lease = JobLease::open(root.path(), games.path(), Some(capture.clone())).unwrap();
        assert_eq!(lease.run(&reqwest::Client::new(), &TokenManager::with_memory()).await, ExitCode::FAILURE);
        assert_eq!(*capture.0.lock().unwrap(), Some("sign-in-required"));
        let result: Value = serde_json::from_slice(&std::fs::read(root.path().join("job-result.json")).unwrap()).unwrap();
        assert_eq!(result["detail"]["code"], "sign-in-required");
    }
    const A: &str = "9AAAAAAAAAAA";
    const B: &str = "9BBBBBBBBBBB";
    fn request() -> Value { json!({"schema":1,"requestId":"11111111-1111-4111-8111-111111111111","operation":"refresh","market":"GB"}) }
    fn row() -> Value { json!({"status":"Active","isTrial":false,"sharingSource":"None","special":false,"endDate":"2099-01-01T00:00:00Z"}) }
    #[test] fn requests_reject_paths_unknown_fields_and_unbound_installs() {
        let valid = request(); assert!(parse(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for (key,value) in [("destination",json!("/tmp/elsewhere")),("productId",json!("../escape")),("operation",json!("install")),("requestId",json!("old")),("accountKey",json!("token")),("market",json!("GB&x=y"))] {
            let mut v = valid.clone(); v[key] = value; assert!(parse(&serde_json::to_vec(&v).unwrap()).is_err(),"{key}");
        }
        let mut v = valid; v["operation"] = json!("install"); v["accountKey"] = json!("a".repeat(64)); v["productId"] = json!(A);
        assert!(parse(&serde_json::to_vec(&v).unwrap()).is_ok());
        v["operation"] = json!("plan");
        assert!(parse(&serde_json::to_vec(&v).unwrap()).is_ok());
        v.as_object_mut().unwrap().remove("accountKey");
        assert!(parse(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    #[test] fn access_rejects_cycles_expiry_trials_and_missing_non_subscription_parents() {
        let mut rows = BTreeMap::from([(A.into(),row())]); assert!(entitled(A,&rows,0));
        for (key,value) in [("status",json!("Revoked")),("isTrial",json!(true)),("special",json!(true)),("sharingSource",json!("Family")),("endDate",json!("1970-01-01T00:00:00Z")),("satisfiedByProductId",json!(B)),("satisfiedByProductId",json!(A))] {
            let mut r = row(); r[key] = value; rows.insert(A.into(),r); assert!(!entitled(A,&rows,0),"{key}");
        }
        let mut r = row(); r["satisfiedByProductId"] = json!(PLANS[0]); rows.insert(A.into(),r);
        assert!(entitled(A,&rows,0)); let mut revoked = row(); revoked["status"] = json!("Revoked"); rows.insert(PLANS[0].into(),revoked); assert!(!entitled(A,&rows,0));
    }
    #[test] fn signin_test_requires_bound_pc_package_and_rejects_active_account_inheritance() {
        let mut value=request(); value["operation"]=json!("signin-test");
        assert!(parse(&serde_json::to_vec(&value).unwrap()).is_err());
        value["productId"]=json!(A);
        assert!(parse(&serde_json::to_vec(&value).unwrap()).is_ok());
        value["contentId"]=json!("11111111-1111-4111-8111-111111111111");
        assert!(parse(&serde_json::to_vec(&value).unwrap()).is_ok());
        value["accountKey"]=json!("a".repeat(64));
        assert!(parse(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    #[tokio::test] async fn native_signout_clears_all_user_credentials_without_deleting_games_or_device_identity() {
        let root=tempfile::tempdir().unwrap();let games=tempfile::tempdir().unwrap();
        let tokens=TokenManager::with_memory();
        tokens.save_device_license(&xodus::models::secrets::Device{puid:"synthetic-device".into(),username:"device-only".into(),
            password:"synthetic-device-secret".into(),hwid:"stable-hardware".into(),device_id:"keep-device".into(),splicense:"synthetic-device-license".into()}).unwrap();
        tokens.save_device_token(xodus::tokens::PASSPORT_STS.into(),xodus::models::secrets::Token::Compact("synthetic-device-token".into())).unwrap();
        tokens.save_user_session(xodus::models::secrets::User{puid:"synthetic".into(),username:"synthetic".into()},
            std::collections::HashMap::from([(xodus::tokens::PASSPORT_STS.into(),xodus::models::secrets::Token::Compact("synthetic-user-token".into()))])).unwrap();
        std::fs::write(root.path().join("catalog.json"),"synthetic-catalog").unwrap();
        std::fs::write(root.path().join("prepared-games.json"),"keep-installed-records").unwrap();
        std::fs::write(games.path().join("save-file"),"keep-saves").unwrap();
        let req=parse(&serde_json::to_vec(&json!({"schema":1,"requestId":"11111111-1111-4111-8111-111111111111","operation":"signout","market":"GB"})).unwrap()).unwrap();
        execute(&reqwest::Client::new(),&tokens,root.path(),games.path(),&req).await.unwrap();
        assert!(tokens.get_user().is_err() && tokens.get_user_sts_token().is_err());
        assert!(tokens.get_device_sts_token().is_err());
        assert_eq!(tokens.get_device_license().unwrap().device_id,"keep-device");
        assert!(!root.path().join("catalog.json").exists());
        assert_eq!(std::fs::read_to_string(root.path().join("prepared-games.json")).unwrap(),"keep-installed-records");
        assert_eq!(std::fs::read_to_string(games.path().join("save-file")).unwrap(),"keep-saves");
        let mut bad=json!({"schema":1,"requestId":req.id,"operation":"signout","market":"GB","productId":A});
        assert!(parse(&serde_json::to_vec(&bad).unwrap()).is_err());
        bad.as_object_mut().unwrap().remove("productId");bad["accountKey"]=json!("a".repeat(64));
        assert!(parse(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    #[test] fn catalog_never_accepts_foreign_products_or_authenticated_image_urls() {
        let p = json!({"ProductId":A,"ProductType":"Game","LocalizedProperties":[{"ProductTitle":"Example","Images":[{"ImagePurpose":"BoxArt","Uri":"https://secret@store-images.s-microsoft.com/a"}]}]});
        let rows = catalog_rows(&json!({"Products":[p.clone()]}),&[A.into()]).unwrap(); assert!(rows[0]["artwork"].is_null());
        assert!(catalog_rows(&json!({"Products":[p.clone()]}),&[B.into()]).is_err());
        assert!(catalog_rows(&json!({"Products":[p.clone(),p]}),&[A.into()]).is_err());
    }
    #[test] fn atomic_results_keep_request_identity_and_bounded_reads() {
        let dir = tempfile::tempdir().unwrap(); let req = parse(&serde_json::to_vec(&request()).unwrap()).unwrap();
        state(dir.path(),&req,"checking-account",json!({})).unwrap(); state(dir.path(),&req,"failed",json!({"code":"account-changed"})).unwrap();
        let result: Value = serde_json::from_slice(&read(&dir.path().join("job-result.json"),4096).unwrap()).unwrap();
        assert_eq!(result["requestId"],req.id); assert_eq!(result["status"],"failed"); assert!(read(&dir.path().join("job-result.json"),8).is_err());
    }
    #[test] fn executable_selection_rejects_escape_ambiguity_and_non_pc_entries() {
        assert_eq!(declared_program(br#"<Game><ExecutableList><Executable Name="Game.exe"/></ExecutableList></Game>"#).unwrap(), "Game.exe");
        assert_eq!(declared_program(br#"<Game><ExecutableList><Executable Name="Tools\Game.exe"/><Executable Name="Dev.exe" IsDevOnly="true"/></ExecutableList></Game>"#).unwrap(), "Tools/Game.exe");
        for xml in [br#"<Game><ExecutableList><Executable Name="../escape.exe"/></ExecutableList></Game>"#.as_slice(),
            br#"<Game><ExecutableList><Executable Name="Game.exe"/><Executable Name="Other.exe"/></ExecutableList></Game>"#,
            br#"<Game><ExecutableList><Executable Name="Game.exe" TargetDeviceFamily="XboxOne"/></ExecutableList></Game>"#,
            br#"<!DOCTYPE Game><Game><ExecutableList><Executable Name="Game.exe"/></ExecutableList></Game>"#,
            br#"<Game><Unexpected><Executable Name="Game.exe"/></Unexpected></Game>"#] { assert!(declared_program(xml).is_err()); }
    }
}

#[cfg(test)]
#[test]
fn embedded_phase_is_emitted_only_after_durable_publication() {
    use crate::job_observer::JobObserver;
    use std::sync::{Arc, Mutex};
    struct Capture { root: std::path::PathBuf, phases: Mutex<Vec<String>> }
    impl JobObserver for Capture {
        fn phase(&self, phase: &str) {
            let record: Value = serde_json::from_slice(&std::fs::read(self.root.join("job-result.json")).unwrap()).unwrap();
            assert_eq!(record["status"],phase);
            self.phases.lock().unwrap().push(phase.into());
        }
    }
    let root = tempfile::tempdir().unwrap();
    let capture = Arc::new(Capture { root: root.path().into(), phases: Mutex::new(Vec::new()) });
    let mut request = parse(br#"{"schema":1,"requestId":"11111111-1111-1111-1111-111111111111","operation":"refresh","market":"GB"}"#).unwrap();
    request.observer = Some(capture.clone());
    state(root.path(),&request,"refreshing",json!({"private":"not part of callback"})).unwrap();
    assert_eq!(*capture.phases.lock().unwrap(),vec!["refreshing"]);
    assert!(state(&root.path().join("missing"),&request,"complete",json!({})).is_err());
    assert_eq!(capture.phases.lock().unwrap().len(),1);
}
