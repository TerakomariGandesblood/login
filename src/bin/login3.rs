use std::{
    fs::{self, OpenOptions},
    sync::Arc,
    thread,
    time::Duration,
};

use anyhow::Result;
use reqwest::{Certificate, Client, Proxy, redirect};
use serde::{Deserialize, Serialize};

struct HttpClient {
    client: Client,
}

#[derive(Serialize, Deserialize, Debug)]
struct QueryResponse {
    status: String,
    result: Query,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum Query {
    OK(Vec<QueryData>),
    Error(QueryError),
}

#[derive(Serialize, Deserialize, Debug)]
struct QueryData {
    staffName: String,
    idCardNo: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct QueryError {
    errorCode: String,
    errorMsg: String,
}

impl HttpClient {
    fn build() -> Result<Self> {
        // let cert = Certificate::from_pem(&fs::read(
        //     "/Users/terakomari/.mitmproxy/mitmproxy-ca-cert.pem",
        // )?)?;

        let client = reqwest::Client::builder()
            .redirect(redirect::Policy::none())
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(60))
            // .proxy(Proxy::all("http://127.0.0.1:8080")?)
            // .tls_certs_merge([cert])
            .build()?;

        Ok(Self { client })
    }

    async fn query(&self, org_code: u32) -> Result<Vec<QueryData>> {
        let response: QueryResponse = self
            .client
            .get("https://phjr.abchina.com.cn/corpormbank-openapi-web/comb/quota/getStaffsByEimo")
            .query(&sonic_rs::json!({
                "orgCode":org_code
            }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        if response.status == "SUCCESS" {
            match response.result {
                Query::OK(query_datas) => return Ok(query_datas),
                Query::Error(query_error) => anyhow::bail!("返回失败：{:?}", query_error),
            }
        } else {
            match response.result {
                Query::OK(query_datas) => anyhow::bail!("请求失败：{}", response.status),
                Query::Error(query_error) => anyhow::bail!("请求失败：{:?}", query_error),
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = login::init_log(&clap_verbosity_flag::Verbosity::new(4, 0), "log", "login3")?;
    let client = Arc::new(HttpClient::build()?);

    let result_file = OpenOptions::new()
        .write(true)
        .create(true)
        .open("result.csv")?;
    let mut wtr = csv::Writer::from_writer(result_file);

    for item in client.query(228211).await? {
        wtr.serialize(&item)?;
    }
    wtr.flush()?;

    for i in 1..228211 {
        let org_code = 228211 + i;
        match client.query(org_code).await {
            Ok(result) => {
                tracing::info!("org_code :{org_code} 成功");
                for item in result {
                    wtr.serialize(&item)?;
                }
                wtr.flush()?;
            }
            Err(error) => {
                tracing::error!("{error}");
            }
        }

        thread::sleep(Duration::from_secs(10));

        let org_code = 228211 - i;
        match client.query(org_code).await {
            Ok(result) => {
                tracing::info!("org_code :{org_code} 成功");
                for item in result {
                    wtr.serialize(&item)?;
                }
                wtr.flush()?;
            }
            Err(error) => {
                tracing::error!("{error}");
            }
        }

        thread::sleep(Duration::from_secs(10));
    }

    Ok(())
}
