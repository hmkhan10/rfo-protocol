use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use crate::compiler::{calculate_quality_score, compile_doc, compile_mdoc};
use crate::core_file::{CoreCompiler, CoreFile};
use crate::opt_resolver::OptResolver;
use crate::parser::{extract_coordinates, parse_html, parse_markdown};

// ── CLI Definition ─────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "rfo",
    about = "RFO Protocol CLI — Compile, serve, and inspect AI-optimized content",
    version,
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Compile a .md or .html file into .doc and .mdoc payloads
    Compile {
        /// Path to the source file
        file: PathBuf,

        /// Output directory (defaults to same as input)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Print results to stdout instead of writing files
        #[arg(short, long)]
        pretty: bool,
    },

    /// Watch a directory and auto-recompile on changes
    Watch {
        /// Directory to watch
        dir: PathBuf,

        /// Output directory for compiled assets
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Serve a directory as RFO endpoints (no database required)
    Serve {
        /// Directory containing .md/.html files
        dir: PathBuf,

        /// Port to listen on
        #[arg(short, long, default_value = "3000")]
        port: u16,

        /// Bind address
        #[arg(short, long, default_value = "0.0.0.0")]
        bind: String,
    },

    /// Start the native RFO binary transport server (TCP, no HTTP)
    ServeNative {
        /// Port to listen on
        #[arg(short, long, default_value = "9000")]
        port: u16,

        /// Bind address
        #[arg(short, long, default_value = "127.0.0.1")]
        bind: String,
    },

    /// Run a virtual handshake against any domain
    Inspect {
        /// Target domain URL
        domain: String,

        /// Request .doc or .mdoc payload
        #[arg(short = 't', long, default_value = "mdoc")]
        payload: String,

        /// Pretty-print the response
        #[arg(short = 'p', long)]
        pretty: bool,
    },

    /// Scan a directory and show quality scores for all files
    Audit {
        /// Directory to scan
        dir: PathBuf,
    },

    /// Compile a .core file — the complete intelligence bundle for a website
    Core {
        #[command(subcommand)]
        action: CoreAction,
    },

    /// Manage .opt domains in the native resolver registry
    Opt {
        #[command(subcommand)]
        action: OptAction,
    },
}

#[derive(Subcommand)]
pub enum CoreAction {
    /// Compile a directory of HTML/MD files into a .core file
    Compile {
        /// Domain name (e.g., mysite.opt)
        domain: String,

        /// Directory containing .html/.md files
        dir: PathBuf,

        /// Output .core file path (defaults to {domain}.core.json)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Pretty-print the core file to stdout instead of writing
        #[arg(short, long)]
        pretty: bool,
    },

    /// Inspect a .core file and print its intelligence summary
    Inspect {
        /// Path to the .core.json file
        file: PathBuf,
    },
}

#[derive(Subcommand)]
pub enum OptAction {
    /// Register a .opt domain with its .core file
    Register {
        /// .opt domain name (e.g., mysite.opt)
        domain: String,

        /// Path to the .core.json file
        core: PathBuf,
    },

    /// Resolve a .opt domain and print its .core intelligence
    Resolve {
        /// .opt domain name (e.g., mysite.opt)
        domain: String,

        /// Path to a persistent registry file (optional)
        #[arg(short, long)]
        registry: Option<PathBuf>,
    },

    /// List all registered .opt domains
    List {
        /// Path to a persistent registry file (optional)
        #[arg(short, long)]
        registry: Option<PathBuf>,
    },

    /// Unregister a .opt domain
    Unregister {
        /// .opt domain name (e.g., mysite.opt)
        domain: String,

        /// Path to a persistent registry file (optional)
        #[arg(short, long)]
        registry: Option<PathBuf>,
    },
}

// ── Command Handlers ───────────────────────────────────────────────────────

pub async fn execute(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Commands::Compile {
            file,
            output,
            pretty,
        } => cmd_compile(&file, output.as_deref(), pretty)?,
        Commands::Watch { dir, output } => cmd_watch(&dir, output.as_deref()).await?,
        Commands::Serve { dir, port, bind } => cmd_serve(&dir, &bind, port).await?,
        Commands::ServeNative { port, bind } => cmd_serve_native(&bind, port).await?,
        Commands::Inspect {
            domain,
            payload,
            pretty,
        } => cmd_inspect(&domain, &payload, pretty).await?,
        Commands::Audit { dir } => cmd_audit(&dir)?,
        Commands::Core { action } => cmd_core(action).await?,
        Commands::Opt { action } => cmd_opt(action)?,
    }
    Ok(())
}

// ── compile ────────────────────────────────────────────────────────────────

fn cmd_compile(
    file: &Path,
    output_dir: Option<&Path>,
    pretty: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(file)
        .map_err(|e| format!("Failed to read {}: {}", file.display(), e))?;

    let is_markdown = file
        .extension()
        .map(|ext| ext == "md" || ext == "markdown")
        .unwrap_or(false);

    let parsed = if is_markdown {
        parse_markdown(&content)
    } else {
        parse_html(&content)
    };

    let domain_url = format!("file://{}", file.display());
    let mdoc = compile_mdoc(&parsed);
    let doc = compile_doc(&parsed, &domain_url);
    let quality_score = calculate_quality_score(&mdoc, &doc);
    let coordinates = extract_coordinates(&parsed);

    if pretty {
        println!("═══ RFO Compile Results ═══");
        println!("Source:    {}", file.display());
        println!("Type:      {}", if is_markdown { "Markdown" } else { "HTML" });
        println!("Quality:   {}", quality_score);
        println!("Tokens:    {} (.mdoc)", mdoc.token_count);
        println!("QaPairs:   {}", mdoc.qa_pairs.len());
        println!("Markdown:  {} chars (.doc)", doc.raw_markdown.len());
        println!("Tables:    {}", doc.data_tables.len());
        println!("Coords:    {:?}", coordinates);
        println!();
        println!("─── .mdoc Summary ───");
        println!("{}", mdoc.summary.chars().take(500).collect::<String>());
        if mdoc.summary.len() > 500 {
            println!("... ({} more chars)", mdoc.summary.len() - 500);
        }
    } else {
        // Write .doc and .mdoc JSON files
        let out = output_dir.unwrap_or(file.parent().unwrap_or(Path::new(".")));
        std::fs::create_dir_all(out)?;

        let stem = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");

        let doc_path = out.join(format!("{}.doc.json", stem));
        let mdoc_path = out.join(format!("{}.mdoc.json", stem));

        std::fs::write(&doc_path, serde_json::to_string_pretty(&doc)?)?;
        std::fs::write(&mdoc_path, serde_json::to_string_pretty(&mdoc)?)?;

        println!("Compiled: {}", file.display());
        println!("  .doc  → {}", doc_path.display());
        println!("  .mdoc → {}", mdoc_path.display());
        println!("  quality: {}", quality_score);
    }

    Ok(())
}

// ── watch ──────────────────────────────────────────────────────────────────

async fn cmd_watch(
    dir: &Path,
    output_dir: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    use notify::{Event, EventKind, RecursiveMode, Watcher};

    println!("Watching {} for changes...", dir.display());
    println!("Output: {}", output_dir.unwrap_or(dir).display());
    println!("Press Ctrl+C to stop.\n");

    // Initial compile of all files
    let compiled = compile_directory(dir, output_dir)?;
    println!("Initial scan: {} files compiled\n", compiled.len());

    // Set up file watcher
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res: Result<Event, _>| {
        if let Ok(event) = res {
            let _ = tx.send(event);
        }
    })?;

    watcher.watch(dir, RecursiveMode::Recursive)?;

    // Process events
    loop {
        match rx.recv() {
            Ok(event) => {
                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) => {
                        for path in &event.paths {
                            if is_compilable(path) {
                                match compile_single(path, output_dir) {
                                    Ok((quality, tokens)) => {
                                        println!(
                                            "[RECOMPILE] {} → quality={}, tokens={}",
                                            path.display(),
                                            quality,
                                            tokens
                                        );
                                        // Re-compile full entry for cache
                                        let content = std::fs::read_to_string(path).unwrap_or_default();
                                        let is_md = path.extension().map(|e| e == "md" || e == "markdown").unwrap_or(false);
                                        let parsed = if is_md { parse_markdown(&content) } else { parse_html(&content) };
                                        let domain_url = format!("file://{}", path.display());
                                        let mdoc = compile_mdoc(&parsed);
                                        let doc = compile_doc(&parsed, &domain_url);
                                        compiled.insert(path.to_string_lossy().to_string(), (doc, mdoc, quality));
                                    }
                                    Err(e) => {
                                        eprintln!("[ERROR] {}: {}", path.display(), e);
                                    }
                                }
                            }
                        }
                    }
                    EventKind::Remove(_) => {
                        for path in &event.paths {
                            let key = path.to_string_lossy().to_string();
                            if compiled.remove(&key).is_some() {
                                println!("[REMOVED] {}", path.display());
                            }
                        }
                    }
                    _ => {}
                }
            }
            Err(e) => {
                eprintln!("Watch error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

// ── serve ──────────────────────────────────────────────────────────────────

async fn cmd_serve(
    dir: &Path,
    bind: &str,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    use axum::routing::get;
    use axum::{Json, Router};

    println!("RFO Directory Server");
    println!("  Serving: {}", dir.display());
    println!("  Listen:  {}:{}\n", bind, port);

    // Pre-compile all files
    let cache = std::sync::Arc::new(dashmap::DashMap::new());
    let entries = compile_directory(dir, None)?;
    for entry in entries.iter() {
        let (doc, mdoc, score) = entry.value();
        cache.insert(
            entry.key().clone(),
            (doc.clone(), mdoc.clone(), *score),
        );
    }

    let cache_health = cache.clone();
    let cache_files = cache.clone();
    let cache_doc = cache.clone();

    let app = Router::new()
        .route("/rfo/health", get(move || async move {
            Json(serde_json::json!({"status": "healthy", "mode": "serve", "files": cache_health.len()}))
        }))
        .route(
            "/rfo/files",
            get(move || async move {
                let files: Vec<serde_json::Value> = cache_files
                    .iter()
                    .map(|entry| {
                        serde_json::json!({
                            "path": entry.key(),
                            "quality_score": entry.value().2,
                            "doc_chars": entry.value().0.raw_markdown.len(),
                            "mdoc_tokens": entry.value().1.token_count,
                        })
                    })
                    .collect();
                Json(files)
            }),
        )
        .route(
            "/rfo/doc/{file}",
            get(move |axum::extract::Path(file): axum::extract::Path<String>| async move {
                match cache_doc.get(&file) {
                    Some(entry) => Json(serde_json::to_value(&entry.value().0).unwrap_or_default()),
                    None => Json(serde_json::json!({"error": "file not found"})),
                }
            }),
        );

    let addr = format!("{}:{}", bind, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("Listening on http://{}", addr);
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}

// ── serve-native ──────────────────────────────────────────────────────────

async fn cmd_serve_native(
    bind: &str,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::transport::{NativeTransport, TransportConfig};

    let addr: std::net::SocketAddr = format!("{}:{}", bind, port).parse()?;
    let config = TransportConfig {
        bind_addr: addr,
        ..Default::default()
    };
    let transport = NativeTransport::new(config);

    println!("RFO Native Binary Transport");
    println!("  Protocol: rfo-binary-v1 (TCP, no HTTP)");
    println!("  Listen:   {}", addr);
    println!();
    println!("AI models connect directly via NativeClient or rfo_opt_resolve() FFI.");
    println!("Supported frame types:");
    println!("  0x10 HANDSHAKE    — negotiate connection");
    println!("  0x11 RESOLVE_OPT  — resolve .opt domain → .core file");
    println!("  0x12 CORE_FILE    — .core intelligence bundle response");
    println!();

    transport.serve().await?;

    Ok(())
}

// ── inspect ────────────────────────────────────────────────────────────────

async fn cmd_inspect(
    domain: &str,
    _payload_type: &str,
    pretty: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = if domain.starts_with("http://") || domain.starts_with("https://") {
        domain.to_string()
    } else {
        format!("https://{}", domain)
    };

    println!("Inspecting {}...\n", url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let start = std::time::Instant::now();
    let response = client.get(&url).send().await?;
    let fetch_time = start.elapsed().as_millis();
    let status = response.status();
    let content = response.text().await?;

    println!("Status:     {}", status);
    println!("Fetch time: {}ms", fetch_time);
    println!("Body size:  {} bytes\n", content.len());

    let is_markdown = url.ends_with(".md");
    let parsed = if is_markdown {
        parse_markdown(&content)
    } else {
        parse_html(&content)
    };

    let mdoc = compile_mdoc(&parsed);
    let doc = compile_doc(&parsed, &url);
    let quality_score = calculate_quality_score(&mdoc, &doc);
    let coordinates = extract_coordinates(&parsed);
    let site_id = crate::crypto::site_id::generate_site_id(&url).unwrap_or_default();

    println!("═══ RFO Virtual Handshake ═══");
    println!("Site ID:    {}... (64 chars)", &site_id[..16]);
    println!("Quality:    {}", quality_score);
    println!("Title:      {}", parsed.title);
    println!("Headings:   {}", parsed.headings.len());
    println!("Paragraphs: {}", parsed.paragraphs.len());
    println!("Code blocks:{}", parsed.code_blocks.len());
    println!("Tables:     {}", parsed.tables.len());
    println!("Coordinates:{:?}", coordinates);
    println!();
    println!("─── .mdoc Payload ───");
    println!("Tokens:  {}", mdoc.token_count);
    println!("QaPairs: {}", mdoc.qa_pairs.len());
    println!();
    println!("─── .doc Payload ───");
    println!("Markdown: {} chars", doc.raw_markdown.len());
    println!("Tables:   {}", doc.data_tables.len());
    println!("Sig:      {}... (64 chars)", &doc.verification_signature[..16]);

    if pretty {
        println!();
        println!("─── QaPairs ───");
        for (i, qa) in mdoc.qa_pairs.iter().enumerate() {
            println!("Q{}: {}", i + 1, qa.question);
            println!("   {}", qa.answer.chars().take(200).collect::<String>());
            println!();
        }
    }

    Ok(())
}

// ── audit ──────────────────────────────────────────────────────────────────

fn cmd_audit(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("RFO Content Audit: {}\n", dir.display());
    println!("{:<50} {:>8} {:>8} {:>10}", "File", "Quality", "Tokens", "QaPairs");
    println!("{}", "─".repeat(80));

    let mut total_quality = 0u64;
    let mut count = 0u64;

    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !is_compilable(path) {
            continue;
        }

        match compile_single(path, None) {
            Ok((quality, tokens)) => {
                let name = path
                    .strip_prefix(dir)
                    .unwrap_or(path)
                    .display()
                    .to_string();
                println!("{:<50} {:>8} {:>8} {:>10}", name, quality, tokens, "-");
                total_quality += quality as u64;
                count += 1;
            }
            Err(e) => {
                let name = path.display().to_string();
                println!("{:<50} {:>8}", name, format!("ERR: {}", e));
            }
        }
    }

    if count > 0 {
        let avg = total_quality / count;
        println!();
        println!("Total files: {}", count);
        println!("Avg quality: {}", avg);
    } else {
        println!("\nNo compilable files found (.md, .html, .htm)");
    }

    Ok(())
}

// ── core ───────────────────────────────────────────────────────────────────

async fn cmd_core(action: CoreAction) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        CoreAction::Compile {
            domain,
            dir,
            output,
            pretty,
        } => {
            if !dir.exists() {
                return Err(format!("Directory not found: {}", dir.display()).into());
            }

            println!("Compiling .core for {} from {}...", domain, dir.display());
            let compiler = CoreCompiler::new();
            let core = compiler.compile_from_directory(&domain, &dir)?;
            let json = serde_json::to_string_pretty(&core)?;

            if pretty {
                println!();
                println!("═══ RFO .core File ═══");
                println!("Schema:    {}", core.schema);
                println!("Domain:    {}", core.site.domain);
                println!("Site ID:   {}...", &core.site.site_id[..16]);
                println!("Pages:     {}", core.site.total_pages);
                println!("Overall:   {}", core.quality.overall);
                println!("Avg Page:  {}", core.quality.avg_page as u32);
                println!("Total Tok: {}", core.quality.total_tokens);
                println!("Total QAs: {}", core.quality.total_qa_pairs);
                println!("AEO Ready: {}", core.quality.aeo_readiness);
                println!("Topics:    {}", core.intelligence.topics.len());
                if !core.intelligence.topics.is_empty() {
                    for t in &core.intelligence.topics {
                        println!("  • {} (confidence: {}) — {} pages",
                            t.name, t.confidence, t.page_urls.len());
                    }
                }
                println!();
                println!("─── Pages ───");
                for p in &core.pages {
                    println!("  {:<50} quality={} tokens={} qas={}",
                        p.path.chars().take(48).collect::<String>(),
                        p.quality_score, p.token_count, p.qa_pair_count);
                }
                println!();
                println!("─── Full JSON ───");
                println!("{}", json);
            } else {
                let out_path = output.unwrap_or_else(|| {
                    let safe_name = domain.replace('.', "_").replace('/', "_");
                    std::path::PathBuf::from(format!("{}.core.json", safe_name))
                });
                std::fs::write(&out_path, &json)?;
                println!(".core written to {}", out_path.display());
                println!("  Pages:     {}", core.site.total_pages);
                println!("  Overall:   {}", core.quality.overall);
                println!("  Tokens:    {}", core.quality.total_tokens);
                println!("  QAs:       {}", core.quality.total_qa_pairs);
            }

            Ok(())
        }

        CoreAction::Inspect { file } => {
            if !file.exists() {
                return Err(format!("File not found: {}", file.display()).into());
            }
            let content = std::fs::read_to_string(&file)?;
            let core: CoreFile = serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse .core file: {}", e))?;

            println!("═══ RFO .core Intelligence Report ═══");
            println!("File:      {}", file.display());
            println!("Schema:    {}", core.schema);
            println!("Version:   {}", core.version);
            println!("Compiled:  {}", core.compiled_at);
            println!();
            println!("─── Site Identity ───");
            println!("Domain:    {}", core.site.domain);
            println!("Site ID:   {}", core.site.site_id);
            println!("Type:      {}", if core.site.is_opt { ".opt" } else { "standard" });
            println!("Title:     {}", core.site.title);
            println!("Pages:     {}", core.site.total_pages);
            println!("URL:       {}", core.site.site_url);
            println!();
            println!("─── Quality ───");
            println!("Overall:   {}", core.quality.overall);
            println!("Average:   {}", core.quality.avg_page as u32);
            println!("Best:      {} ({})", core.quality.best_page, core.quality.best_score);
            println!("Worst:     {} ({})", core.quality.worst_page, core.quality.worst_score);
            println!("Total Tok: {}", core.quality.total_tokens);
            println!("Total QAs: {}", core.quality.total_qa_pairs);
            println!("AEO Ready: {}", core.quality.aeo_readiness);
            println!();
            println!("─── Intelligence ───");
            println!("Topics:");
            for t in &core.intelligence.topics {
                println!("  • {} (confidence: {}) — {} pages",
                    t.name, t.confidence, t.page_urls.len());
            }
            println!("Site Summary: {}...",
                core.intelligence.site_summary.chars().take(200).collect::<String>());
            println!();
            println!("─── Pages ({}) ───", core.pages.len());
            for p in &core.pages {
                let depth_marker = "  ".repeat(p.depth.min(5));
                println!("{}{} ({}, {} tok, {} qas)",
                    depth_marker, p.path, p.quality_score, p.token_count, p.qa_pair_count);
            }
            println!();
            println!("─── Crypto ───");
            println!("Site ID Sig: {}...", &core.crypto.site_id_signature[..16]);
            println!("Content Hash: {}...", &core.crypto.content_root_hash[..16]);
            println!("Page Hashes:  {}", core.crypto.page_hashes.len());
            println!("Verified:     {}", core.crypto.verified);
            println!();
            println!("─── Optimization ───");
            println!("SEO:   title={}", core.optimization.seo.title);
            println!("GEO:   llm_friendly={}, lang={}",
                core.optimization.geo.llm_friendly, core.optimization.geo.language);
            println!("AEO:   qa_pairs={}, snippets={}, faq={}",
                core.optimization.aeo.has_qa_pairs,
                core.optimization.aeo.featured_snippets,
                core.optimization.aeo.faq_schema);

            Ok(())
        }
    }
}

// ── opt ─────────────────────────────────────────────────────────────────────

fn cmd_opt(action: OptAction) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        OptAction::Register { domain, core } => {
            let json = std::fs::read_to_string(&core)?;
            let core_file: CoreFile = serde_json::from_str(&json)?;
            let resolver = OptResolver::new();
            resolver.register(&domain, core_file)?;
            println!("Registered {} → {}", domain, core.display());
            println!("Total .opt domains: {}", resolver.count());
            Ok(())
        }
        OptAction::Resolve {
            domain,
            registry,
        } => {
            let resolver = if let Some(path) = &registry {
                OptResolver::load_from_file(path)?
            } else {
                OptResolver::new()
            };
            match resolver.resolve(&domain) {
                Ok(cf) => {
                    println!("═══ .opt Domain: {} ═══", domain);
                    println!("Site ID:     {}", cf.site.site_id);
                    println!("Title:       {}", cf.site.title);
                    println!("Description: {}", cf.site.description);
                    println!("Pages:       {}", cf.site.total_pages);
                    println!("Tokens:      {}", cf.intelligence.site_token_count);
                    println!("QA Pairs:    {}", cf.intelligence.all_qa_pairs.len());
                    println!("Topics:      {}", cf.intelligence.topics.len());
                    println!("Quality:     {}", cf.quality.overall);
                    println!("AEO:         {}", cf.quality.aeo_readiness);
                    println!("Verified:    {}", cf.crypto.verified);
                    println!("Compiled:    {}", cf.compiled_at);
                    Ok(())
                }
                Err(e) => Err(format!("Failed to resolve '{}': {}", domain, e).into()),
            }
        }
        OptAction::List { registry } => {
            let resolver = if let Some(path) = &registry {
                OptResolver::load_from_file(path)?
            } else {
                OptResolver::new()
            };
            let domains = resolver.list();
            if domains.is_empty() {
                println!("No .opt domains registered.");
            } else {
                println!("Registered .opt domains ({}):", resolver.count());
                for d in &domains {
                    let meta = resolver.resolve_metadata(d).unwrap_or_else(|_| unreachable!());
                    println!("  {}  (resolved {} times, last: {})",
                        d,
                        meta.resolve_count,
                        meta.last_resolved.format("%Y-%m-%d %H:%M:%S"),
                    );
                }
            }
            Ok(())
        }
        OptAction::Unregister {
            domain,
            registry,
        } => {
            let resolver = if let Some(path) = &registry {
                OptResolver::load_from_file(path)?
            } else {
                OptResolver::new()
            };
            resolver.unregister(&domain)?;
            println!("Unregistered {}", domain);
            Ok(())
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn is_compilable(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext, "md" | "markdown" | "html" | "htm"))
        .unwrap_or(false)
}

fn compile_single(
    file: &Path,
    output_dir: Option<&Path>,
) -> Result<(u32, usize), Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(file)?;
    let is_markdown = file
        .extension()
        .map(|ext| ext == "md" || ext == "markdown")
        .unwrap_or(false);

    let parsed = if is_markdown {
        parse_markdown(&content)
    } else {
        parse_html(&content)
    };

    let domain_url = format!("file://{}", file.display());
    let mdoc = compile_mdoc(&parsed);
    let doc = compile_doc(&parsed, &domain_url);
    let quality_score = calculate_quality_score(&mdoc, &doc);

    // Optionally write output files
    if let Some(out) = output_dir {
        std::fs::create_dir_all(out)?;
        let stem = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        std::fs::write(
            out.join(format!("{}.doc.json", stem)),
            serde_json::to_string(&doc)?,
        )?;
        std::fs::write(
            out.join(format!("{}.mdoc.json", stem)),
            serde_json::to_string(&mdoc)?,
        )?;
    }

    Ok((quality_score, mdoc.token_count))
}

type CompileResult = (crate::rfo_protocol::FullDocPayload, crate::rfo_protocol::MiniDocPayload, u32);

fn compile_directory(
    dir: &Path,
    _output_dir: Option<&Path>,
) -> Result<dashmap::DashMap<String, CompileResult>, Box<dyn std::error::Error>> {
    let results = dashmap::DashMap::new();

    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !is_compilable(path) {
            continue;
        }

        let content = std::fs::read_to_string(path)?;
        let is_markdown = path
            .extension()
            .map(|ext| ext == "md" || ext == "markdown")
            .unwrap_or(false);

        let parsed = if is_markdown {
            parse_markdown(&content)
        } else {
            parse_html(&content)
        };

        let domain_url = format!("file://{}", path.display());
        let mdoc = compile_mdoc(&parsed);
        let doc = compile_doc(&parsed, &domain_url);
        let quality_score = calculate_quality_score(&mdoc, &doc);

        results.insert(
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string(),
            (doc, mdoc, quality_score),
        );
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn setup() {
        std::env::set_var("RFO_SECRET_KEY", "test-cli-secret-key");
    }

    #[test]
    fn test_compile_markdown_file() {
        setup();
        let dir = std::env::temp_dir().join("rfo_test_compile");
        let _ = std::fs::create_dir_all(&dir);

        let file = dir.join("test.md");
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, "# Test Page\n\nThis is a test paragraph about Rust programming.").unwrap();

        let result = compile_single(&file, None);
        assert!(result.is_ok());

        let (quality, tokens) = result.unwrap();
        assert!(quality > 0);
        assert!(tokens > 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_compile_html_file() {
        setup();
        let dir = std::env::temp_dir().join("rfo_test_html");
        let _ = std::fs::create_dir_all(&dir);

        let file = dir.join("test.html");
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, "<html><head><title>Test</title></head><body><h1>Hello</h1><p>World</p></body></html>").unwrap();

        let result = compile_single(&file, None);
        assert!(result.is_ok());

        let (quality, tokens) = result.unwrap();
        assert!(quality > 0);
        assert!(tokens > 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_is_compilable() {
        assert!(is_compilable(Path::new("test.md")));
        assert!(is_compilable(Path::new("test.html")));
        assert!(is_compilable(Path::new("test.htm")));
        assert!(is_compilable(Path::new("test.markdown")));
        assert!(!is_compilable(Path::new("test.txt")));
        assert!(!is_compilable(Path::new("test.rs")));
    }

    #[test]
    fn test_compile_directory() {
        setup();
        let dir = std::env::temp_dir().join("rfo_test_dir");
        let _ = std::fs::create_dir_all(&dir);

        std::fs::write(dir.join("a.md"), "# Page A\n\nContent about Rust.").unwrap();
        std::fs::write(dir.join("b.md"), "# Page B\n\nContent about Python.").unwrap();
        std::fs::write(dir.join("c.txt"), "Not compilable").unwrap();

        let results = compile_directory(&dir, None).unwrap();
        assert_eq!(results.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
