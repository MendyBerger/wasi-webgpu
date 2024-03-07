use std::sync::Mutex;

use crate::{
    wasi::webgpu::key_events::{KeyEvent, Pollable},
    HostEvent, HostState,
};
use tokio::sync::broadcast::Receiver;
use wasmtime::component::Resource;
use wasmtime_wasi::preview2::{self, WasiView};

impl crate::wasi::webgpu::key_events::Host for HostState {
    fn up_listener(&mut self) -> wasmtime::Result<Resource<KeyUpListener>> {
        let receiver = self.sender.subscribe();
        Ok(self
            .table_mut()
            .push(KeyUpListener {
                receiver,
                data: Default::default(),
            })
            .unwrap())
    }

    fn down_listener(&mut self) -> wasmtime::Result<Resource<KeyDownListener>> {
        let receiver = self.sender.subscribe();
        Ok(self
            .table_mut()
            .push(KeyDownListener {
                receiver,
                data: Default::default(),
            })
            .unwrap())
    }
}

impl crate::wasi::webgpu::key_events::HostKeyUpListener for HostState {
    fn subscribe(
        &mut self,
        key_up: Resource<KeyUpListener>,
    ) -> wasmtime::Result<Resource<Pollable>> {
        Ok(preview2::subscribe(self.table_mut(), key_up).unwrap())
    }
    fn get(&mut self, key_up: Resource<KeyUpListener>) -> wasmtime::Result<Option<KeyEvent>> {
        let key_up = self.table.get(&key_up).unwrap();
        Ok(key_up.data.lock().unwrap().take())
    }
    fn drop(&mut self, _self_: Resource<KeyUpListener>) -> wasmtime::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct KeyUpListener {
    receiver: Receiver<HostEvent>,
    data: Mutex<Option<KeyEvent>>,
}

#[async_trait::async_trait]
impl preview2::Subscribe for KeyUpListener {
    async fn ready(&mut self) {
        loop {
            let event = self.receiver.recv().await.unwrap();
            if let HostEvent::KeyUpEvent(event) = event {
                *self.data.lock().unwrap() = Some(event);
                return;
            }
        }
    }
}

impl crate::wasi::webgpu::key_events::HostKeyDownListener for HostState {
    fn subscribe(
        &mut self,
        key_down: Resource<KeyDownListener>,
    ) -> wasmtime::Result<Resource<Pollable>> {
        Ok(preview2::subscribe(self.table_mut(), key_down).unwrap())
    }
    fn get(&mut self, key_down: Resource<KeyDownListener>) -> wasmtime::Result<Option<KeyEvent>> {
        let key_down = self.table.get(&key_down).unwrap();
        Ok(key_down.data.lock().unwrap().take())
    }
    fn drop(&mut self, _self_: Resource<KeyDownListener>) -> wasmtime::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct KeyDownListener {
    receiver: Receiver<HostEvent>,
    data: Mutex<Option<KeyEvent>>,
}

#[async_trait::async_trait]
impl preview2::Subscribe for KeyDownListener {
    async fn ready(&mut self) {
        loop {
            let event = self.receiver.recv().await.unwrap();
            if let HostEvent::KeyDownEvent(event) = event {
                *self.data.lock().unwrap() = Some(event);
                return;
            }
        }
    }
}
