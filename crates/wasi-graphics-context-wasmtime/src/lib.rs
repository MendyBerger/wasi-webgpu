use std::any::Any;

use crate::wasi::webgpu::graphics_context::{self, ConfigureContextDesc};
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle, RawDisplayHandle, RawWindowHandle};
use wasmtime::component::Resource;
use wasmtime_wasi::preview2::WasiView;

pub use wasi::webgpu::graphics_context::add_to_linker;

wasmtime::component::bindgen!({
    path: "../../wit/",
    world: "example",
    async: false,
    with: {
        "wasi:webgpu/graphics-context/graphics-context": GraphicsContext,
        "wasi:webgpu/graphics-context/graphics-context-buffer": GraphicsContextBuffer,
    },
});

pub struct GraphicsContext {
    draw_api: Option<Box<dyn DrawApi + Send + Sync>>,
    display_api: Option<Box<dyn DisplayApi + Send + Sync>>,
}

impl GraphicsContext {
    pub fn new() -> Self {
        Self {
            display_api: None,
            draw_api: None,
        }
    }

    pub fn configure(&mut self, _desc: ConfigureContextDesc) -> wasmtime::Result<()> {
        Ok(())
    }

    pub fn connect_display_api(&mut self, display_api: Box<dyn DisplayApi + Send + Sync>) {
        if let Some(draw_api) = &mut self.draw_api {
            draw_api.display_api_ready(&display_api)
        }
        self.display_api = Some(display_api);
    }

    // pub fn resize(&mut self, height: u32, width: u32) {
    //     self.height = Some(height);
    //     self.width = Some(width);
    // }

    pub fn connect_draw_api(&mut self, mut draw_api: Box<dyn DrawApi + Send + Sync>) {
        if let Some(display_api) = &self.display_api {
            draw_api.display_api_ready(&*display_api)
        }
        self.draw_api = Some(draw_api);
    }
}

// TODO: rename to FrameProvider? since this isn't neceraly implemented on the whole api?
pub trait DrawApi {
    fn get_current_buffer(&mut self) -> wasmtime::Result<GraphicsContextBuffer>;
    fn present(&mut self) -> wasmtime::Result<()>;
    fn display_api_ready(&mut self, display_api: &Box<dyn DisplayApi + Send + Sync>);
}

pub trait DisplayApi: HasRawDisplayHandle + HasRawWindowHandle {
    fn height(&self) -> u32;
    fn width(&self) -> u32;
    fn display_handle(&self) -> DisplayHandle {
        DisplayHandle(self.raw_display_handle())
    }
    fn window_handle(&self) -> WindowHandle {
        WindowHandle(self.raw_window_handle())
    }
}

pub struct DisplayHandle(RawDisplayHandle);
unsafe impl Send for DisplayHandle {}
unsafe impl Sync for DisplayHandle {}
impl DisplayHandle {
    pub fn get(self) -> RawDisplayHandle {
        self.0
    }
}
pub struct WindowHandle(pub RawWindowHandle);
unsafe impl Send for WindowHandle {}
unsafe impl Sync for WindowHandle {}
impl WindowHandle {
    pub fn get(self) -> RawWindowHandle {
        self.0
    }
}

pub struct GraphicsContextBuffer {
    buffer: Box<dyn Any + Send + Sync>,
}
impl<T> From<Box<T>> for GraphicsContextBuffer
where
    T: Any + Send + Sync + 'static,
{
    fn from(value: Box<T>) -> Self {
        Self {
            buffer: Box::new(value),
        }
    }
}

impl GraphicsContextBuffer {
    pub fn inner_type<T>(self) -> T
    where
        T: 'static,
    {
        **self.buffer.downcast::<Box<T>>().unwrap()
    }
}

// wasmtime
impl<T: WasiView> graphics_context::Host for T {}

impl<T: WasiView> graphics_context::HostGraphicsContext for T {
    fn new(&mut self) -> wasmtime::Result<Resource<GraphicsContext>> {
        Ok(self.table_mut().push(GraphicsContext::new()).unwrap())
    }

    fn configure(
        &mut self,
        context: Resource<GraphicsContext>,
        desc: ConfigureContextDesc,
    ) -> wasmtime::Result<()> {
        let graphics_context = self.table_mut().get_mut(&context).unwrap();
        graphics_context.configure(desc).unwrap();
        Ok(())
    }

    fn get_current_buffer(
        &mut self,
        context: Resource<GraphicsContext>,
    ) -> wasmtime::Result<Resource<GraphicsContextBuffer>> {
        let context_kind = self.table_mut().get_mut(&context).unwrap();
        let next_frame = context_kind
            .draw_api
            .as_mut()
            .expect("draw_api not set")
            .get_current_buffer()
            .unwrap();
        let next_frame = self.table_mut().push(next_frame).unwrap();
        Ok(next_frame)
    }

    fn present(&mut self, context: Resource<GraphicsContext>) -> wasmtime::Result<()> {
        let context = self.table_mut().get_mut(&context).unwrap();
        // context.display_api.as_mut().unwrap().present().unwrap();
        context.draw_api.as_mut().unwrap().present().unwrap();
        Ok(())
    }

    fn drop(&mut self, _graphics_context: Resource<GraphicsContext>) -> wasmtime::Result<()> {
        // todo!()
        Ok(())
    }
}

impl<T: WasiView> graphics_context::HostGraphicsContextBuffer for T {
    fn drop(&mut self, _rep: Resource<GraphicsContextBuffer>) -> wasmtime::Result<()> {
        todo!()
    }
}
