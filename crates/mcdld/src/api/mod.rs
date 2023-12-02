use reqwest::Response;
use std::{fmt::Debug, fs::File};
mod path;
use eyre::{Report, eyre, WrapErr};
use std::cmp::min;
use std::io::Write;

use futures::stream::{Stream, StreamExt};


#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct DownloadFilePathParams {}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct DownloadFileQueryParams {
    duration: u32,
    size_bytes: u32,
}

#[derive(Debug, Default)]
pub struct Api {
    client: reqwest::Client,
    base_url: String,
    headers: reqwest::header::HeaderMap,
}

impl Api {
    // make headers optional with builder pattern later
    fn new(base_url: String, headers: reqwest::header::HeaderMap) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
            headers: headers,
        }
    }

    /// fn name == alias
    async fn get_latest_release(&self) -> Result<Response, Report> {
        // let path = "post";
        let path = "anything/post/1234";
        let body = serde_json::json!({"asdf":"qwer"});

        let method = reqwest::Method::POST;
        let url = format!("{}/{}", self.base_url, path);
        let req = self
            .client
            .request(method, url)
            .body(body.to_string())
            .build()?;
        let resp = self.client.execute(req).await?;
        Ok(resp)
    }

    /// fn name == alias
    async fn download_file(
        &self,
        path_params: Option<DownloadFilePathParams>,
        query_params: Option<DownloadFileQueryParams>,
    ) -> Result<Response, Report> {
        let path = "drip";
        let method = reqwest::Method::GET;

        let path = match path_params {
            Some(path_params) => path::fmt(path, &path_params).unwrap(),
            None => path.to_string(),
        };

        let url = format!("{}/{}", self.base_url, path);
        let req = self.client.request(method, url);

        let req = match query_params {
            Some(query_params) => req.query(&query_params),
            None => req,
        };

        let req = req.build()?;

        let resp = self.client.execute(req).await?;
        Ok(resp)
    }
}

async fn download_file_that_returns_progress(
    // I wish I could do this withtout adding direct dependecy on Bytes
    // byte_stream: impl Stream<Item = Result<Bytes, Error>>,
    // size: u64,
    path: &str,
    response: Response,
) -> Result<impl Stream<Item = Result<f32, Report>>, Report> {

    let total_size = response
        .content_length()
        .ok_or(eyre!("failed to get content length from"))?;

    let byte_stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut file = File::create(path).wrap_err(eyre!("failed to create file '{}'", path))?;

    let number_stream = byte_stream.map(move |byte_result_item| {
        let item = byte_result_item.map_err(|e| eyre!("error while downloading file: {}", e));
        match item {
            Ok(chunk) => {
                file.write_all(&chunk)
                    .wrap_err("error while writing to file")?;
                
                let new = min(downloaded + (chunk.len() as u64), total_size);

                // TODO this returns 100 twice for some reason

                downloaded = new;
                Ok((downloaded as f32 / total_size as f32) * 100.0)
            }
            Err(e) => Err(e),
        }
    });

    Ok(number_stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_download_file() -> Result<(), Report> {
        // API with non-macro version
        let api = Api::new(
            "https://httpbin.org".to_string(),
            reqwest::header::HeaderMap::new(),
        );
        let resp = api
            .download_file(
                None,
                Some(DownloadFileQueryParams {
                    duration: 5,
                    size_bytes: 50,
                }),
            )
            .await?;

        let progress = download_file_that_returns_progress("dummy", resp).await?;
        // small bug that this prints 100 twice
        progress.map(|result| {
            match result {
                Ok(progress) => {
                    println!("p: {}", progress);
                    // do more stuff
                },
                Err(e) => println!("e: {}", e),
            }
        }).collect::<Vec<_>>().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_download_file_macro_version() -> Result<(), Report> {
        // API with macro version
        let api = Api::new(
            "https://httpbin.org".to_string(),
            reqwest::header::HeaderMap::new(),
            
            // endpoints! {
            //     download_file: GET "/drip" Option<DownloadFileQueryParams> => Response,
            //     get_latest_release: POST "/anything/post/:id" => Response, // or struct LatestRelease {abc: String} ... that implements Deserialize
            // }
        );
        // let resp = api
        //     .download_file(
        //         None,
        //         Some(DownloadFileQueryParams {
        //             duration: 5,
        //             size_bytes: 50,
        //         }),
        //     )
        //     .await?;

        // let resp = api
        //     .get_latest_release(
        //         Some(PathParams {
        //             id: 5,
        //         }),
        //     )
        //     .await?;

        // handle response here

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_latest_release() -> Result<(), Report> {
        let api = Api::new(
            "https://httpbin.org".to_string(),
            reqwest::header::HeaderMap::new(),
        );
        let resp = api.get_latest_release().await?;
        // dbg!(&resp);
        dbg!(serde_json::from_str::<serde_json::Value>(
            &resp.text().await?
        )?);
        Ok(())
    }
}
