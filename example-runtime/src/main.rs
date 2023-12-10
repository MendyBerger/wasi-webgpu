use anyhow::Context;
use clap::Parser;
use wasmtime::{
    component::{Component, Linker},
    Config, Engine, Store,
};

use crate::webgpu_host::WebGpuHost;

mod webgpu_host;

#[derive(clap::Parser, Debug)]
struct RuntimeArgs {
    /// The example name
    #[arg(long)]
    example: String,
}

wasmtime::component::bindgen!({
    path: "../wit/",
    world: "example",
    async: {
        only_imports: [],
    },
});

struct HostState {
    pub web_gpu_host: WebGpuHost<'static>,
}

impl HostState {
    fn new() -> Self {
        Self {
            web_gpu_host: WebGpuHost::new(),
        }
    }
}

#[async_trait::async_trait]
impl ExampleImports for HostState {
    fn print(&mut self, s: String) -> wasmtime::Result<()> {
        println!("{s}");
        Ok(())
    }
}

#[async_std::main]
async fn main() -> anyhow::Result<()> {
    let args = RuntimeArgs::parse();

    let mut config = Config::default();
    config.wasm_component_model(true);
    config.async_support(true);
    let engine = Engine::new(&config)?;
    let mut linker = Linker::new(&engine);

    component::webgpu::webgpu::add_to_linker(&mut linker, |state: &mut HostState| {
        &mut state.web_gpu_host
    })?;

    Example::add_root_to_linker(&mut linker, |state: &mut HostState| state)?;

    let webgpu_host = HostState::new();

    let mut store = Store::new(&engine, webgpu_host);

    let wasm_path = format!("../example-apps/{}/out.wasm", args.example);

    let component =
        Component::from_file(&engine, &wasm_path).context("Component file not found")?;

    let (instance, _) = Example::instantiate_async(&mut store, &component, &linker)
        .await
        .unwrap();

    instance.call_start(&mut store).await.unwrap();

    Ok(())
}