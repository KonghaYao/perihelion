// LSP 集成测试
use perihelion_lsp::config::{LspConfigFile, LspServerConfig};
use perihelion_lsp::pool::LspServerPool;
use std::collections::HashMap;

#[tokio::test]
async fn test_rust_analyzer_startup() {
    // 创建 rust-analyzer 配置
    let mut servers = HashMap::new();
    servers.insert(
        "rust-analyzer".to_string(),
        LspServerConfig {
            name: "rust-analyzer".to_string(),
            command: "rust-analyzer".to_string(),
            args: vec![], // rust-analyzer 不需要 --stdio 参数
            env: None,
            extension_to_language: HashMap::from([(".rs".to_string(), "rust".to_string())]),
            initialization_options: None,
            disabled: None,
            max_restarts: Some(3),
            startup_timeout: Some(30000),
            source: None,
        },
    );

    let config = LspConfigFile {
        lsp_servers: servers,
    };

    // 创建 LSP 服务器池
    let pool = LspServerPool::new("/Users/konghayao/code/ai/perihelion", config);

    // 测试按文件启动服务器
    let result = pool.ensure_server_for_file("src/main.rs").await;
    match &result {
        Ok(_) => println!("rust-analyzer started successfully"),
        Err(e) => {
            println!("Failed to start rust-analyzer: {}", e);

            // 检查服务器状态
            let info = pool.server_info();
            println!("Server info: {:#?}", info);
        }
    }

    assert!(
        result.is_ok(),
        "Failed to start rust-analyzer: {:?}",
        result
    );

    // 验证服务器已就绪
    let server = pool.server_for_file("src/main.rs");
    assert!(server.is_some(), "No server found for .rs files");
    assert!(server.unwrap().is_ready(), "Server is not ready");

    // 关闭服务器
    pool.shutdown().await;
}

#[tokio::test]
async fn test_workspace_symbol_query() {
    let mut servers = HashMap::new();
    servers.insert(
        "rust-analyzer".to_string(),
        LspServerConfig {
            name: "rust-analyzer".to_string(),
            command: "rust-analyzer".to_string(),
            args: vec![],
            env: None,
            extension_to_language: HashMap::from([(".rs".to_string(), "rust".to_string())]),
            initialization_options: None,
            disabled: None,
            max_restarts: Some(3),
            startup_timeout: Some(30000),
            source: None,
        },
    );

    let config = LspConfigFile {
        lsp_servers: servers,
    };
    let pool = LspServerPool::new("/Users/konghayao/code/ai/perihelion", config);

    // 初始化所有服务器
    let result = pool.ensure_initialized().await;
    assert!(result.is_ok(), "Failed to initialize servers: {:?}", result);

    // 获取任意一个就绪的服务器
    let server = pool.any_server();
    assert!(server.is_some(), "No ready server available");

    let server = server.unwrap();

    // 执行 workspace/symbol 查询
    let result = server
        .request(
            "workspace/symbol",
            Some(serde_json::json!({ "query": "LspClient" })),
            10_000,
        )
        .await;

    assert!(
        result.is_ok(),
        "workspace/symbol request failed: {:?}",
        result
    );
    let symbols = result.unwrap();
    println!(
        "Workspace symbols for 'LspClient': {:#}",
        serde_json::to_string_pretty(&symbols).unwrap()
    );

    // 关闭服务器
    pool.shutdown().await;
}

#[tokio::test]
async fn test_document_symbols() {
    let mut servers = HashMap::new();
    servers.insert(
        "rust-analyzer".to_string(),
        LspServerConfig {
            name: "rust-analyzer".to_string(),
            command: "rust-analyzer".to_string(),
            args: vec![],
            env: None,
            extension_to_language: HashMap::from([(".rs".to_string(), "rust".to_string())]),
            initialization_options: None,
            disabled: None,
            max_restarts: Some(3),
            startup_timeout: Some(30000),
            source: None,
        },
    );

    let config = LspConfigFile {
        lsp_servers: servers,
    };
    let pool = LspServerPool::new("/Users/konghayao/code/ai/perihelion", config);

    // 启动服务器
    let result = pool.ensure_server_for_file("src/client.rs").await;
    assert!(result.is_ok(), "Failed to start server: {:?}", result);

    let server = pool.server_for_file("src/client.rs").unwrap();

    // 打开文件
    let uri = format!(
        "file://{}",
        "/Users/konghayao/code/ai/perihelion/perihelion-lsp/src/client.rs"
    );
    let content =
        std::fs::read_to_string("/Users/konghayao/code/ai/perihelion/perihelion-lsp/src/client.rs")
            .unwrap();
    server.did_open(&uri, "rust", &content).await.unwrap();

    // 获取文档符号
    let result = server
        .request(
            "textDocument/documentSymbol",
            Some(serde_json::json!({
                "textDocument": { "uri": uri }
            })),
            10_000,
        )
        .await;

    assert!(
        result.is_ok(),
        "documentSymbol request failed: {:?}",
        result
    );
    let symbols = result.unwrap();
    println!(
        "Document symbols for client.rs: {:#}",
        serde_json::to_string_pretty(&symbols).unwrap()
    );

    // 关闭服务器
    pool.shutdown().await;
}
