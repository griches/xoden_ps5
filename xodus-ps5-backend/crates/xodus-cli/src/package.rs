use xodus::XBOX_LIVE_PACKAGES_PC;
use xodus::models::packagespc::{PackageDetails, PackageResponse};
use xodus::models::secrets::Token;
use xodus::tokens::TokenManager;

/// Fixed diagnostics for the embedded downloader, without response bodies or credentials.
#[derive(Debug)]
struct PackageFailure { code: &'static str, message: &'static str }
impl std::fmt::Display for PackageFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { formatter.write_str(self.message) }
}
impl std::error::Error for PackageFailure {}
fn failure(code: &'static str, message: &'static str) -> Box<dyn std::error::Error> {
    Box::new(PackageFailure { code, message })
}
pub fn failure_code(error: &(dyn std::error::Error + 'static)) -> &'static str {
    error.downcast_ref::<PackageFailure>().map(|error| error.code).unwrap_or("package-query")
}

pub async fn get_content_id(
    client: &reqwest::Client,
    product: String,
    market: Option<String>,
) -> Result<String, Box<dyn std::error::Error>> {
    let region = market.unwrap_or("US".into()).to_uppercase();
    let plan = crate::package_plan::resolve(client, &product.to_uppercase(), &region)
        .await
        .map_err(std::io::Error::other)?;
    Ok(plan.select(None).map_err(std::io::Error::other)?)
}

pub async fn get_packages(
    client: &reqwest::Client,
    tokens: &TokenManager,
    content_id: String,
) -> Result<PackageDetails, Box<dyn std::error::Error>> {
    let dev_token = tokens.get_device_sts_token().map_err(|_| failure("package-device-credentials", "Required Xodus credentials are missing. Please sign in again."))?;
    let Token::Legacy(dev_token) = dev_token else {
        return Err(failure("package-device-ticket", "Invalid device sign-in. Please sign in again."));
    };
    let user_token = tokens.get_user_sts_token().map_err(|_| failure("package-user-credentials", "Required Xodus credentials are missing. Please sign in again."))?;
    let Token::Legacy(legacy) = user_token else {
        return Err(failure("package-user-ticket", "Unsupported user sign-in. Please sign in again."));
    };

    let xsts_token =
        xodus::api::xbox::run(client, dev_token, legacy, "http://update.xboxlive.com").await.map_err(|error| {
            use xodus::api::xbox::SessionError;
            let code=match error { SessionError::Request=>"package-xbox-request",SessionError::Timeout=>"package-xbox-timeout",SessionError::Refused=>"package-xbox-refused",SessionError::Token(_)=>"package-xbox-ticket" };
            failure(code,"Xbox package authentication did not complete. Retry the installation.")
        })?;

    let response = client
        .get(format!(
            "{XBOX_LIVE_PACKAGES_PC}/GetBasePackage/{content_id}"
        ))
        .header("x-xbl-contract-version", "3")
        .header(
            "Authorization",
            xodus::api::xbox::get_xsts_auth_header(xsts_token).map_err(|_| failure("package-auth-header","Xbox package authentication is invalid."))?,
        )
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|_| failure("package-request", "Microsoft package request failed. Check your connection and retry."))?
        .error_for_status()
        .map_err(|error| {
            let code=match error.status().map(|status|status.as_u16()) {Some(401)=>"package-http-401",Some(403)=>"package-http-403",Some(404)=>"package-http-404",Some(429)=>"package-http-429",_=>"package-http"};
            failure(code,"Microsoft refused the package request. Check your sign-in and retry.")
        })?;

    let res: PackageResponse = response.json().await.map_err(|_| failure("package-response", "Microsoft returned an unreadable package response."))?;

    let PackageResponse::Found(package) = res else {
        return Err(failure("package-not-found", "Package was not found, is it owned by the user?"));
    };
    Ok(package)
}

#[cfg(test)]
mod credential_tests {
    use super::*;
    #[tokio::test]
    async fn credential_missing_device_stops_package_request_without_panic_or_network() {
        let tokens = TokenManager::with_memory();
        let error = get_packages(&reqwest::Client::new(), &tokens, "synthetic".into())
            .await
            .unwrap_err();
        assert_eq!(failure_code(error.as_ref()),"package-device-credentials");
        assert_eq!(
            error.to_string(),
            "Required Xodus credentials are missing. Please sign in again."
        );
    }
}
