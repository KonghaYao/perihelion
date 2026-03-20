//! # basic_agent
//!
//! 演示最基础的 Agent 使用：创建 Agent、添加中间件、执行任务

use rust_create_agent::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let llm = MockLLM::always_answer("任务完成！这是我的回答。");

    let agent = AgentExecutor::new(llm)
        .max_iterations(10)
        .add_middleware(Box::new(LoggingMiddleware::new().verbose()));

    let mut state = AgentState::new("/workspace");
    let output = agent.execute(AgentInput::text("请帮我完成这个任务"), &mut state).await?;

    println!("\n=== Agent Output ===");
    println!("Answer: {}", output.text);
    println!("Steps: {}", output.steps);
    println!("Tool calls: {}", output.tool_calls.len());

    Ok(())
}
