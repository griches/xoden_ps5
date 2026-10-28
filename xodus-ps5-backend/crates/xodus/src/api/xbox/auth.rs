use crate::models::xbox::{
    UserAuthProperties, UserAuthRequest, XstsPropertyBag, XstsRequest, XstsResponse,
};

/// Try the original broker ticket first, then the explicit RPS representations.
/// Each representation is validated by Xbox; never change credentials or claims.
pub fn rps_representations(ticket: &str) -> Vec<String> {
    let bare=ticket.strip_prefix("d=").or_else(||ticket.strip_prefix("t=")).unwrap_or(ticket);
    let mut output=vec![ticket.to_owned()];
    for value in [bare.to_owned(),format!("d={bare}"),format!("t={bare}")] {
        if !output.contains(&value) { output.push(value); }
    }
    output
}

pub async fn authenticate_xbox_user(
    client: &reqwest::Client,
    rps_ticket: String,
) -> reqwest::Result<XstsResponse> {
    authenticate_xbox_user_with_diagnostics(client, rps_ticket, &mut Vec::new()).await
}

/// Only numeric rejection metadata may leave the authentication layer.
#[derive(Clone, serde::Serialize)]
pub struct UserAuthRejection {
    pub http: u16,
    pub xerr: Option<u64>,
    pub kind: &'static str,
}

fn rejection_code(bytes: &[u8]) -> Option<u64> {
    serde_json::from_slice::<serde_json::Value>(bytes).ok()?.get("XErr")?.as_u64()
}

pub async fn authenticate_xbox_user_with_diagnostics(
    client: &reqwest::Client,
    rps_ticket: String,
    diagnostics: &mut Vec<UserAuthRejection>,
) -> reqwest::Result<XstsResponse> {
    let body = UserAuthRequest {
        relying_party: "http://auth.xboxlive.com".to_string(),
        token_type: "JWT".to_string(),
        properties: UserAuthProperties {
            auth_method: "RPS".to_string(),
            site_name: "user.auth.xboxlive.com".to_string(),
            rps_ticket,
        },
    };

    let response = client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .header("Content-Type", "application/json")
        .header("x-xbl-contract-version", "1")
        .json(&body)
        .send()
        .await;
    let mut resp=match response {
        Ok(response)=>response,
        Err(error)=>{
            diagnostics.push(UserAuthRejection { http:0,xerr:None,
                kind:if error.is_timeout(){"timeout"}else if error.is_connect(){"connect"}else{"request"} });
            return Err(error);
        }
    };

    if let Err(error) = resp.error_for_status_ref() {
        let http = resp.status().as_u16();
        let mut bytes = Vec::new();
        while let Ok(Some(chunk)) = resp.chunk().await {
            if bytes.len() + chunk.len() > 16 * 1024 { bytes.clear(); break; }
            bytes.extend_from_slice(&chunk);
        }
        diagnostics.push(UserAuthRejection { http, xerr: rejection_code(&bytes), kind:"http" });
        return Err(error);
    }

    let result=resp.json().await;
    if result.is_err(){diagnostics.push(UserAuthRejection {http:200,xerr:None,kind:"response"});}
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_ticket_representations_are_bounded_and_keep_the_original_first() {
        for original in ["synthetic", "d=synthetic", "t=synthetic"] {
            let values=rps_representations(original);
            assert_eq!(values[0],original);
            assert_eq!(values.len(),3);
            assert!(values.iter().any(|value|value=="synthetic"));
            assert!(values.iter().any(|value|value=="d=synthetic"));
            assert!(values.iter().any(|value|value=="t=synthetic"));
            assert!(values.iter().all(|value|!value.contains("=d=")&&!value.contains("=t=")));
        }
    }
    #[test]
    fn xbox_rejection_diagnostics_accept_only_numeric_codes() {
        assert_eq!(rejection_code(br#"{"XErr":2148916233,"Message":"private"}"#),Some(2148916233));
        for bytes in [br#"{"XErr":"private"}"#.as_slice(),b"private",br#"{"XErr":-1}"#] {
            assert_eq!(rejection_code(bytes),None);
        }
    }
}

pub async fn request_xsts_token(
    client: &reqwest::Client,
    token: String,
    relying_party: &str,
) -> reqwest::Result<XstsResponse> {
    let body = XstsRequest {
        relying_party: Some(relying_party.to_string()),
        token_type: Some("JWT".to_string()),
        properties: XstsPropertyBag {
            user_tokens: Some(vec![token]),
            sandbox_id: Some("RETAIL".to_string()),
            delegation_token: None,
            service_token: None,
        },
    };

    let resp = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .header("Content-Type", "application/json")
        .header("x-xbl-contract-version", "1")
        .json(&body)
        .send()
        .await?
        .error_for_status()?;

    resp.json().await
}

pub fn get_xsts_auth_header(
    xsts: XstsResponse,
) -> Result<String, crate::models::secrets::InvalidTokenResponse> {
    use crate::models::secrets::InvalidTokenResponse;
    let uhs = xsts
        .user_hash()
        .filter(|value| !value.is_empty())
        .ok_or(InvalidTokenResponse)?;
    if xsts.token.is_empty() {
        return Err(InvalidTokenResponse);
    }
    Ok(format!("XBL3.0 x={uhs};{}", xsts.token))
}
