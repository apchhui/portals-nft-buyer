use reqwest::{Client, header};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::File,
    io::{self, BufRead},
    sync::{Arc},
    time::{Duration, Instant},
};
use once_cell::sync::Lazy;
use tokio::task::JoinHandle;
use tokio::sync::Notify;
use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::{task, time::sleep};
use tokio::sync::{Semaphore, RwLock, Mutex};
use chrono::Local;

type SharedFloors = Arc<RwLock<HashMap<String, f64>>>;
type SentIds = Arc<RwLock<HashSet<String>>>;

static IS_PURCHASING: Lazy<RwLock<bool>> = Lazy::new(|| RwLock::new(false));

#[derive(Deserialize)]
#[derive(Debug)]
struct Nft {
    id: String,
    name: String,
    price: String,
    #[serde(default)]
    collection_slug: Option<String>,
}

#[derive(Deserialize)]
struct ApiResponse {
    results: Vec<Nft>,
}

#[derive(Deserialize)]
struct FloorsResponse {
    #[serde(rename = "floorPrices")]
    floor_prices: HashMap<String, String>,
}

#[derive(Serialize)]
struct NftDetails<'a> {
    id: &'a str,
    price: f64,
}

#[derive(Serialize)]
struct NftPayload<'a> {
    nft_details: Vec<NftDetails<'a>>,
}

type WorkerRegistry = Arc<Mutex<HashMap<usize, JoinHandle<()>>>>;

fn current_time_string() -> String {
    Local::now().format("[%H:%M:%S%.3f]").to_string()
}

async fn set_purchasing(val: bool) {
    let mut flag = IS_PURCHASING.write().await;
    *flag = val;
}

async fn get_purchasing() -> bool {
    *IS_PURCHASING.read().await
}

pub async fn send_telegram_message(message: &str) -> Result<(), Box<dyn Error>> {
    let token = "key";
    let chat_id = "-id";

    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage?chat_id={}&text={}",
        token,
        chat_id,
        urlencoding::encode(message)
    );

    let client = Client::new();

    let res = client.get(&url).send().await?;

    let status = res.status();
    let body = res.text().await.unwrap_or_default();

    if status.is_success() {
        println!("✅ Telegram message sent successfully.");
    } else {
        eprintln!("❌ Failed to send Telegram message. Status: {}, Body: {}", status, body);
    }

    Ok(())
}

async fn post_nft(
    client: &Client,
    auth_header: &str,
    cookie_header: &str,
    id: &str,
    price: f64
) -> Result<(), Box<dyn std::error::Error>> {
    let url = "https://portals-market.com/api/nfts";

    let payload = NftPayload {
        nft_details: vec![NftDetails { id, price }],
    };

    let res = client
        .post(url)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, auth_header)
        .header(header::USER_AGENT, "Mozilla/5.0")
        .header(header::COOKIE, cookie_header)
        .json(&payload)
        .send()
        .await?;

    let status = res.status();
    let text = res.text().await.unwrap_or_default();

    println!("{} | POST Status: {}", current_time_string(), status);
    println!("{} | POST Response body:\n{}", current_time_string(), text);

    let msg = format!(
        "POST Status: {}\nPOST Response body:\n{}",
        status, text
    );
    println!("{}", price);

    set_purchasing(false).await;

    // send_telegram_message(&msg).await?;

    Ok(())
}


fn load_auth_headers(path: &str) -> io::Result<Vec<String>> {
    let file = File::open(path)?;
    let reader = io::BufReader::new(file);
    let headers: Vec<String> = serde_json::from_reader(reader)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(headers)
}

async fn floor_updater(
    client: Client,
    auth_header: String,
    cookie_header: String,
    floors: SharedFloors,
) {
    loop {
        match client
            .get("https://portals-market.com/api/collections/floors")
            .header(header::AUTHORIZATION, &auth_header)
            .header(header::USER_AGENT, "Mozilla/5.0")
            .header(header::COOKIE, &cookie_header)
            .header(header::CONTENT_TYPE, "application/json")
            .send()
            .await
        {
            Ok(res) => {
                if let Ok(body) = res.text().await {
                    match serde_json::from_str::<FloorsResponse>(&body) {
                        Ok(parsed) => {
                            let mut map = HashMap::new();
                            for (k, v) in parsed.floor_prices {
                                if let Ok(val) = v.parse::<f64>() {
                                    map.insert(k.to_lowercase(), val);
                                }
                            }

                            let mut write_guard = floors.write().await;
                            *write_guard = map;

                            println!("{} | Updated floor prices.", current_time_string());
                        }
                        Err(e) => {
                            eprintln!("Failed to parse floors: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Error updating floors: {}", e);
            }
        }

        sleep(Duration::from_secs(30)).await;
    }
}

async fn worker(
    id: usize,
    client: Client,
    auth_header: String,
    cookie_header: String,
    floors: SharedFloors,
    counter: Arc<AtomicUsize>,
    sent_ids: SentIds,
    post_lock: Arc<Mutex<()>>,
    registry: WorkerRegistry,
    notify: Arc<Notify>,
) {

    if get_purchasing().await {
        return;
    }

    let handle = tokio::spawn(async {});
    registry.lock().await.insert(id, handle);

    match client
        .get("https://portals-market.com/api/nfts/search?offset=0&limit=20&status=listed")
        .header(header::AUTHORIZATION, &auth_header)
        .header(header::USER_AGENT, "Mozilla/5.0")
        .header(header::COOKIE, &cookie_header)
        .header(header::CONTENT_TYPE, "application/json")
        .send()
        .await
    {
        Ok(res) => {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            if status.is_success() {
                let api_response: ApiResponse = match serde_json::from_str(&text) {
                    Ok(json) => json,
                    Err(e) => {
                        eprintln!("JSON parse error: {}. Raw body:\n{}", e, text);
                        return;
                    }
                };

                let floors_map = floors.read().await;

                for nft in api_response.results {
                    let price = nft.price.parse::<f64>().unwrap_or(0.0);
                    let slug = nft.name
                        .to_lowercase()
                        .chars()
                        .filter(|c| c.is_ascii_alphabetic())
                        .collect::<String>();

                    if let Some(&floor_price) = floors_map.get(&slug) {
                        if price <= floor_price * 0.9 && floor_price > 0.0 {
                            let mut sent_guard = sent_ids.write().await;
                            if sent_guard.contains(&nft.id) {
                                continue;
                            }
                            sent_guard.insert(nft.id.clone());
                            drop(sent_guard);

                                                    // notify.notify_waiters(); // wake any waiting workers (optional)
                            if get_purchasing().await {
                                return;
                            }

                            set_purchasing(true).await;

                            println!("{} | Triggering POST for NFT: {} at price: {} (floor: {})", current_time_string(), nft.name, price, floor_price);

                            // let _guard = post_lock.lock().await;

                            // let reg = registry.lock().await;
                            // for (&other_id, handle) in reg.iter() {
                            //     if other_id != id {
                            //         handle.abort();
                            //     }
                            // }
                            // drop(reg);

                            post_nft(&client, &auth_header, &cookie_header, &nft.id, price)
                                .await
                                .expect("Failed to post NFT");

                            break;
                        }
                    }
                }

                counter.fetch_add(1, Ordering::Relaxed);
            }
        }
        Err(e) => {
            eprintln!("Request error: {}", e);
        }
    }
}


async fn sent_ids_cleaner(sent_ids: SentIds) {
    loop {
        sleep(Duration::from_secs(60)).await;
        let mut guard = sent_ids.write().await;
        guard.clear();
        println!("{} | Cleared sent NFT IDs", current_time_string());
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cookie_header = r#"cf_clearance="#;
    let auth_headers = load_auth_headers("auth.txt")?;
    if auth_headers.is_empty() {
        eprintln!("No auth headers found in auth.txt");
        return Ok(());
    }

    let client = Client::new();
    let floors: SharedFloors = Arc::new(RwLock::new(HashMap::new()));
    let counter = Arc::new(AtomicUsize::new(0));
    let sent_ids: SentIds = Arc::new(RwLock::new(HashSet::new()));
    let post_lock = Arc::new(Mutex::new(())); 
    let registry: WorkerRegistry = Arc::new(Mutex::new(HashMap::new()));
    let notify = Arc::new(Notify::new());

    let token_queue = Arc::new(Mutex::new(VecDeque::from_iter(auth_headers.into_iter().enumerate().map(|(i, h)| (h, Instant::now() - Duration::from_secs(1), i)))));

    {
        let counter = Arc::clone(&counter);
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(1)).await;
                let c = counter.swap(0, Ordering::Relaxed);
                println!("{} | Requests per second: {}", current_time_string(), c);
            }
        });
    }

    {
        let floors_clone = Arc::clone(&floors);
        let client_clone = client.clone();
        let cookie = cookie_header.to_string();
        let token_queue = Arc::clone(&token_queue);
        let first_token = {
            let q = token_queue.lock().await;
            q.front().map(|(h, _, _)| h.clone()).unwrap_or_default()
        };

        tokio::spawn(floor_updater(client_clone, first_token, cookie, floors_clone));
    }

    {
        let client_clone = client.clone();
        let floors_clone = Arc::clone(&floors);
        let token_queue = Arc::clone(&token_queue);
        let cookie = cookie_header.to_string();
        let counter_clone = Arc::clone(&counter);
        let sent_ids_clone = Arc::clone(&sent_ids);
        let post_lock_clone = Arc::clone(&post_lock);
        let registry_clone = Arc::clone(&registry);
        let notify_clone = Arc::clone(&notify);

        tokio::spawn(async move {
            let mut id_counter = 0;

            loop {
                sleep(Duration::from_millis(5)).await;

                let (token, index) = {
                    let mut queue = token_queue.lock().await;
                    if let Some(pos) = queue.iter().position(|(_, last, _)| last.elapsed() >= Duration::from_millis(333)) {
                        let (token, _, idx) = queue.remove(pos).unwrap();
                        queue.push_back((token.clone(), Instant::now(), idx));
                        (token, idx)
                    } else {
                        continue;
                    }
                };

                let client = client_clone.clone();
                let floors = Arc::clone(&floors_clone);
                let cookie = cookie.clone();
                let counter = Arc::clone(&counter_clone);
                let sent_ids = Arc::clone(&sent_ids_clone);
                let post_lock = Arc::clone(&post_lock_clone);
                let registry = Arc::clone(&registry_clone);
                let notify = Arc::clone(&notify_clone);
                let id = id_counter;
                id_counter += 1;

                tokio::spawn(async move {
                    worker(id, client, token, cookie, floors, counter, sent_ids, post_lock, registry, notify).await;
                });
            }
        });
    }

    loop {
        sleep(Duration::from_secs(3600)).await;
    }
}
