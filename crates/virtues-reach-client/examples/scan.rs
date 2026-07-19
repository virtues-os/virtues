#[tokio::main]
async fn main() {
    let t = std::time::Instant::now();
    let boxes = virtues_reach_client::scan_subnet().await;
    println!("found {} box(es) in {:?}: {:#?}", boxes.len(), t.elapsed(), boxes);
}
