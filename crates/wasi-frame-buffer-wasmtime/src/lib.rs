use std::mem;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

use wasmtime::component::Resource;
use wasmtime_wasi::preview2::WasiView;

use crate::wasi::webgpu::frame_buffer;
use wasi_graphics_context_wasmtime::{DisplayApi, DrawApi, GraphicsContext, GraphicsContextBuffer};

pub use wasi::webgpu::frame_buffer::add_to_linker;

wasmtime::component::bindgen!({
    path: "../../wit/",
    world: "example",
    async: {
        only_imports: [],
    },
    with: {
        "wasi:webgpu/frame-buffer/surface": FBSurfaceArc,
        "wasi:webgpu/frame-buffer/frame-buffer": FBBuffer,
        "wasi:webgpu/graphics-context": wasi_graphics_context_wasmtime,
    },
});

pub struct FBSurface {
    pub(crate) surface: Option<softbuffer::Surface>,
}
// TODO: actually ensure safety
unsafe impl Send for FBSurface {}
unsafe impl Sync for FBSurface {}
impl FBSurface {
    pub fn new() -> Self {
        Self { surface: None }
    }
}

// TODO: can we avoid the Mutex here?
pub struct FBSurfaceArc(pub Arc<Mutex<FBSurface>>);
impl FBSurfaceArc {
    pub fn new() -> Self {
        FBSurfaceArc(Arc::new(Mutex::new(FBSurface::new())))
    }
}
impl DrawApi for FBSurfaceArc {
    fn get_current_buffer(&mut self) -> wasmtime::Result<GraphicsContextBuffer> {
        self.0.lock().unwrap().get_current_buffer()
    }

    fn present(&mut self) -> wasmtime::Result<()> {
        self.0.lock().unwrap().present()
    }

    fn display_api_ready(&mut self, display_api: &Box<dyn DisplayApi + Send + Sync>) {
        self.0.lock().unwrap().display_api_ready(display_api)
    }
}

// impl Surface {
//     pub fn resize(&mut self, width: NonZeroU32, height: NonZeroU32) {
//         self.surface.lock().unwrap().resize(width, height).unwrap();
//     }
// }

impl DrawApi for FBSurface {
    fn get_current_buffer(&mut self) -> wasmtime::Result<GraphicsContextBuffer> {
        let surface = self.surface.as_mut().unwrap();
        let buff = surface.buffer_mut().unwrap();
        // TODO: use ouroboros?
        let buff: softbuffer::Buffer<'static> = unsafe { mem::transmute(buff) };
        let buff: FBBuffer = buff.into();
        let buff = Box::new(buff);
        let buff: GraphicsContextBuffer = buff.into();
        Ok(buff)
    }

    fn present(&mut self) -> wasmtime::Result<()> {
        self.surface
            .as_mut()
            .unwrap()
            .buffer_mut()
            .unwrap()
            .present()
            .unwrap();
        Ok(())
    }

    fn display_api_ready(&mut self, display: &Box<dyn DisplayApi + Send + Sync>) {
        let context =
            unsafe { softbuffer::Context::from_raw(display.raw_display_handle()) }.unwrap();
        let mut surface =
            unsafe { softbuffer::Surface::from_raw(&context, display.raw_window_handle()) }
                .unwrap();

        // softbuffer requires setting the size before presenting.
        let _ = surface.resize(
            display
                .width()
                .try_into()
                .unwrap_or(NonZeroU32::new(1).unwrap()),
            display
                .height()
                .try_into()
                .unwrap_or(NonZeroU32::new(1).unwrap()),
        );
        self.surface = Some(surface);
    }
}

pub struct FBBuffer {
    // Never none
    buffer: Arc<Mutex<Option<softbuffer::Buffer<'static>>>>,
}
// TODO: ensure safety
unsafe impl Send for FBBuffer {}
unsafe impl Sync for FBBuffer {}
impl From<softbuffer::Buffer<'static>> for FBBuffer {
    fn from(buffer: softbuffer::Buffer<'static>) -> Self {
        FBBuffer {
            buffer: Arc::new(Mutex::new(Some(buffer))),
        }
    }
}

// wasmtime
impl<T: WasiView> frame_buffer::Host for T {}

impl<T: WasiView> frame_buffer::HostSurface for T {
    fn new(&mut self) -> wasmtime::Result<Resource<crate::wasi::webgpu::frame_buffer::Surface>> {
        Ok(self.table_mut().push(FBSurfaceArc::new()).unwrap())
    }

    fn connect_graphics_context(
        &mut self,
        surface: Resource<FBSurfaceArc>,
        graphics_context: Resource<GraphicsContext>,
    ) -> wasmtime::Result<()> {
        let surface = FBSurfaceArc(Arc::clone(&self.table().get(&surface).unwrap().0));
        let graphics_context = self.table_mut().get_mut(&graphics_context).unwrap();
        graphics_context.connect_draw_api(Box::new(surface));
        Ok(())
    }

    fn drop(&mut self, _rep: Resource<FBSurfaceArc>) -> wasmtime::Result<()> {
        todo!()
    }
}

impl<T: WasiView> frame_buffer::HostFrameBuffer for T {
    // impl<T: WasiView> frame_buffer::HostFrameBuffer for T {
    fn from_graphics_buffer(
        &mut self,
        buffer: Resource<GraphicsContextBuffer>,
    ) -> wasmtime::Result<Resource<FBBuffer>> {
        let host_buffer: GraphicsContextBuffer = self.table_mut().delete(buffer).unwrap();
        let host_buffer: FBBuffer = host_buffer.inner_type();
        Ok(self.table_mut().push(host_buffer).unwrap())
    }

    fn length(&mut self, buffer: Resource<FBBuffer>) -> wasmtime::Result<u32> {
        let buffer = self.table().get(&buffer).unwrap();
        let len = buffer.buffer.lock().unwrap().as_ref().unwrap().len();
        Ok(len as u32)
    }

    fn get(&mut self, buffer: Resource<FBBuffer>, i: u32) -> wasmtime::Result<u32> {
        let buffer = self.table().get(&buffer).unwrap();
        let val = *buffer
            .buffer
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .get(i as usize)
            .unwrap();
        Ok(val)
    }

    fn set(&mut self, buffer: Resource<FBBuffer>, i: u32, val: u32) -> wasmtime::Result<()> {
        let buffer = self.table_mut().get_mut(&buffer).unwrap();
        buffer.buffer.lock().unwrap().as_mut().unwrap()[i as usize] = val as u32;
        Ok(())
    }

    fn drop(&mut self, frame_buffer: Resource<FBBuffer>) -> wasmtime::Result<()> {
        let frame_buffer = self.table_mut().delete(frame_buffer).unwrap();
        frame_buffer.buffer.lock().unwrap().take();
        Ok(())
    }
}