use ows_examples::{run_listen_workflow, ExampleResult};

#[tokio::main]
async fn main() -> ExampleResult<()> {
    run_listen_workflow().await
}
