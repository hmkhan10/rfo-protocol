use rfo_core::client::RfoClient;
use rfo_core::rfo_protocol::PayloadType;
use std::collections::HashMap;

#[tokio::main]
async fn main() {
    let client = RfoClient::new("http://localhost:3000");

    // 1. Health check
    match client.health_check().await {
        Ok(health) => println!("✓ Health: {}", health),
        Err(e) => println!("✗ Health: {}", e),
    }

    // 2. Handshake (mdoc)
    let mut coords = HashMap::new();
    coords.insert("category".to_string(), "Testing".to_string());
    coords.insert("language".to_string(), "Rust".to_string());

    match client.handshake("https://httpbin.org/html", coords, PayloadType::Mdoc).await {
        Ok(resp) => {
            println!(
                "✓ Handshake: site_id={}... score={} time={}ms",
                &resp.header.site_id[..16],
                resp.header.quality_score,
                resp.processing_time_ms
            );
        }
        Err(e) => println!("✗ Handshake: {}", e),
    }

    // 3. Cache hit
    let coords2 = HashMap::new();
    match client
        .handshake("https://httpbin.org/html", coords2, PayloadType::Mdoc)
        .await
    {
        Ok(resp) => println!("✓ Cached: time={}ms", resp.processing_time_ms),
        Err(e) => println!("✗ Cached: {}", e),
    }

    // 4. get_mdoc
    match client.get_mdoc("httpbin.org").await {
        Ok(mdoc) => println!(
            "✓ .mdoc: {} tokens, {} QaPairs",
            mdoc.token_count,
            mdoc.qa_pairs.len()
        ),
        Err(e) => println!("✗ .mdoc: {}", e),
    }

    // 5. get_doc
    match client.get_doc("httpbin.org").await {
        Ok(doc) => println!("✓ .doc: {} chars markdown", doc.raw_markdown.len()),
        Err(e) => println!("✗ .doc: {}", e),
    }

    // 6. list_sites
    match client.list_sites().await {
        Ok(sites) => println!("✓ Sites: {} registered", sites.len()),
        Err(e) => println!("✗ Sites: {}", e),
    }

    // 7. Batch handshake
    let domains = vec!["https://httpbin.org/html", "https://example.com"];
    let batch_coords = HashMap::new();
    let results = client
        .batch_handshake(&domains, batch_coords, PayloadType::Mdoc)
        .await;
    for (domain, result) in &results {
        match result {
            Ok(r) => println!("✓ Batch {}: score={}", domain, r.header.quality_score),
            Err(e) => println!("✗ Batch {}: {}", domain, e),
        }
    }
}
