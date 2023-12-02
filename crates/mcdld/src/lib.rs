

use std::cmp::min;
use std::fs::File;
use std::io::Write;




use reqwest::{Client};
// use indicatif::{ProgressBar, ProgressStyle};
// use futures_util::StreamExt;



use futures::stream::{Stream, StreamExt};


// TODO this should later return Report or something more clever
type MyResult<T> = Result<T, String>;

// download chunks and return stream of numbers
async fn download_file_that_returns_progress(
    client: &Client,
    url: &str,
    path: &str,
) -> Result<impl Stream<Item = MyResult<f32>>, String> {
    // Reqwest setup
    let req_response = client
        .get(url)
        .send()
        .await
        .or(Err(format!("Failed to GET from '{}'", &url)))?;

    
    // downlaod chunks

    let total_size = req_response
        .content_length()
        .ok_or(format!("Failed to get content length from '{}'", &url))?;

    let byte_stream = req_response.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut file = File::create(path).or(Err(format!("Failed to create file '{}'", path)))?;

    let number_stream = byte_stream.map(move |byte_result_item| {
        let item = byte_result_item.map_err(|e| format!("Error while downloading file: {}", e));
        match item {
            Ok(chunk) => {
                file.write_all(&chunk)
                    .or(Err(format!("Error while writing to file")))?;
                
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

pub async fn process_download_progress(path: &str) -> Result<impl Stream<Item = Result<f32, String>>, String> {
    // use futures::future;

    let (client, url) = setup_req()?;
    let progress = download_file_that_returns_progress(&client, &url, path).await?;

    Ok(progress)
}

fn setup_req() -> Result<(Client, String), String>{
     // client: &Client, url: &str, path: &str

    // read GITHUB_TOKEN from env and add it to the headers
    // let token = env::var("GITHUB_TOKEN").or(Err("GITHUB_TOKEN not found"))?;
    let token = "ghp_2Z1Z0Z4Z5Z6Z7Z8Z9ZAZBZCZDZEZFZGZH";
    let b = format!("Bearer {}", token.clone());

    let mut default_headers = reqwest::header::HeaderMap::new();
    default_headers.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::try_from(b)
            .expect("failed to create header value from token"),
    );

    default_headers.insert(
        "X-GitHub-Api-Version",
        reqwest::header::HeaderValue::from_static("2022-11-28"),
    );

    let client = Client::builder()
        .default_headers(default_headers)
        .build()
        .or(Err("Failed to build client"))?;
    // let url = "https://httpbin.org/drip?duration=5&numbytes=50&code=200";
    let url = format!(
        "https://httpbin.org/drip?duration={duration}&numbytes={size_bytes}&code=200",
        duration = 5,
        size_bytes = 50
    );
    // let path = "test.txt";

    Ok(
        (client, url)
    )
}

mod api;