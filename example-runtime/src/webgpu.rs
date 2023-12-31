use futures::executor::block_on;
use std::borrow::Cow;
use std::collections::HashMap;
use std::marker::PhantomPinned;
use std::pin::Pin;
use wasmtime::component::Resource;

use crate::component::webgpu::webgpu;
use crate::graphics_context::{GraphicsBuffer, GraphicsContext, GraphicsContextKind};
use crate::HostState;

pub struct DeviceAndQueue {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,

    // only needed when calling surface.get_capabilities in connect_graphics_context. If table would have a way to get parent from child, we could get it from device.
    pub _adapter: Resource<wgpu::Adapter>,
}

#[async_trait::async_trait]
impl webgpu::Host for HostState {
    async fn request_adapter(&mut self) -> wasmtime::Result<Resource<wgpu::Adapter>> {
        let adapter = block_on(self.instance.request_adapter(&Default::default())).unwrap();
        Ok(self.table.push(adapter).unwrap())
    }
}

#[async_trait::async_trait]
impl webgpu::HostGpuDevice for HostState {
    async fn connect_graphics_context(
        &mut self,
        daq: Resource<DeviceAndQueue>,
        context: Resource<GraphicsContext>,
    ) -> wasmtime::Result<()> {
        let surface = unsafe { self.instance.create_surface(&self.window) }.unwrap();

        let host_daq = self.table.get(&daq).unwrap();

        // think the table should have a way to get parent so that we can get adapter from device.
        let adapter = self.table.get(&host_daq._adapter).unwrap();

        let mut size = self.window.inner_size();
        size.width = size.width.max(1);
        size.height = size.height.max(1);

        let swapchain_capabilities = surface.get_capabilities(&adapter);
        let swapchain_format = swapchain_capabilities.formats[0];

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: swapchain_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: swapchain_capabilities.alpha_modes[0],
            view_formats: vec![],
        };

        surface.configure(&host_daq.device, &config);

        let context = self.table.get_mut(&context).unwrap();

        context.kind = Some(GraphicsContextKind::Webgpu(surface));

        Ok(())
    }

    async fn create_command_encoder(
        &mut self,
        daq: Resource<DeviceAndQueue>,
    ) -> wasmtime::Result<Resource<CommandEncoderWithRenderPass>> {
        let host_daq = self.table.get(&daq).unwrap();
        let command_encoder = host_daq
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let command_encoder = CommandEncoderWithRenderPass::new(command_encoder);

        Ok(self.table.push_child(command_encoder, &daq).unwrap())
    }

    async fn create_shader_module(
        &mut self,
        daq: Resource<DeviceAndQueue>,
        desc: webgpu::GpuShaderModuleDescriptor,
    ) -> wasmtime::Result<Resource<webgpu::GpuShaderModule>> {
        let daq = self.table.get(&daq).unwrap();
        let shader = daq
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: desc.label.as_deref(),
                source: wgpu::ShaderSource::Wgsl(Cow::Owned(desc.code)),
            });

        Ok(self.table.push(shader).unwrap())
    }

    async fn create_render_pipeline(
        &mut self,
        daq: Resource<DeviceAndQueue>,
        props: webgpu::GpuRenderPipelineDescriptor,
    ) -> wasmtime::Result<Resource<webgpu::GpuRenderPipeline>> {
        let vertex = wgpu::VertexState {
            module: &self.table.get(&props.vertex.module).unwrap(),
            entry_point: &props.vertex.entry_point,
            buffers: &[],
        };

        let fragment = wgpu::FragmentState {
            module: &self.table.get(&props.fragment.module).unwrap(),
            entry_point: &props.fragment.entry_point,
            targets: &props
                .fragment
                .targets
                .iter()
                .map(|target| {
                    Some(wgpu::ColorTargetState {
                        format: target.into(),
                        blend: None,
                        write_mask: Default::default(),
                    })
                })
                .collect::<Vec<_>>(),
        };

        // let primitive = wgpu::PrimitiveState {
        //     topology: (&props.primitive.topology).into(),
        //     ..Default::default()
        // };

        let host_daq = self.table.get(&daq).unwrap();

        let render_pipeline =
            host_daq
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    vertex,
                    fragment: Some(fragment),
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: Default::default(),
                    multisample: Default::default(),
                    multiview: Default::default(),
                    label: Default::default(),
                    layout: Default::default(),
                });

        Ok(self.table.push_child(render_pipeline, &daq).unwrap())
    }

    async fn queue(
        &mut self,
        daq: Resource<DeviceAndQueue>,
    ) -> wasmtime::Result<Resource<DeviceAndQueue>> {
        Ok(Resource::new_own(daq.rep()))
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuDevice>) -> wasmtime::Result<()> {
        // self.web_gpu_host.devices.remove(&rep.rep());
        Ok(())
    }
}

#[async_trait::async_trait]
impl webgpu::HostGpuTexture for HostState {
    async fn from_graphics_buffer(
        &mut self,
        buffer: Resource<GraphicsBuffer>,
    ) -> wasmtime::Result<Resource<wgpu::SurfaceTexture>> {
        let host_buffer = self.table.delete(buffer).unwrap();
        if let GraphicsBuffer::Webgpu(host_buffer) = host_buffer {
            Ok(self.table.push(host_buffer).unwrap())
        } else {
            panic!("Context not connected to webgpu");
        }
    }

    async fn create_view(
        &mut self,
        texture: Resource<wgpu::SurfaceTexture>,
    ) -> wasmtime::Result<Resource<wgpu::TextureView>> {
        let host_texture = self.table.get(&texture).unwrap();
        let texture_view = host_texture.texture.create_view(&Default::default());

        Ok(self.table.push(texture_view).unwrap())
    }

    async fn non_standard_present(
        &mut self,
        texture: Resource<wgpu::SurfaceTexture>,
    ) -> wasmtime::Result<()> {
        let texture = self.table.delete(texture).unwrap();
        texture.present();
        Ok(())
    }

    fn drop(&mut self, _rep: Resource<wgpu::SurfaceTexture>) -> wasmtime::Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl webgpu::HostGpuTextureView for HostState {
    fn drop(&mut self, _rep: Resource<wgpu::TextureView>) -> wasmtime::Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl webgpu::HostGpuCommandBuffer for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::GpuCommandBuffer>) -> wasmtime::Result<()> {
        // self.web_gpu_host.command_buffers.remove(&rep.rep());
        Ok(())
    }
}

#[async_trait::async_trait]
impl webgpu::HostGpuShaderModule for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::GpuShaderModule>) -> wasmtime::Result<()> {
        // self.web_gpu_host.shaders.remove(&rep.rep());
        Ok(())
    }
}

#[async_trait::async_trait]
impl webgpu::HostGpuRenderPipeline for HostState {
    fn drop(&mut self, _rep: Resource<webgpu::GpuRenderPipeline>) -> wasmtime::Result<()> {
        // TODO:
        Ok(())
    }
}

#[async_trait::async_trait]
impl webgpu::HostGpuAdapter for HostState {
    async fn request_device(
        &mut self,
        adapter: Resource<wgpu::Adapter>,
    ) -> wasmtime::Result<Resource<webgpu::GpuDevice>> {
        let host_adapter = self.table.get(&adapter).unwrap();

        let (device, queue) =
            block_on(host_adapter.request_device(&Default::default(), Default::default())).unwrap();

        let daq = self
            .table
            .push_child(
                DeviceAndQueue {
                    device,
                    queue,
                    _adapter: Resource::new_own(adapter.rep()),
                },
                &adapter,
            )
            .unwrap();

        Ok(daq)
    }

    fn drop(&mut self, adapter: Resource<webgpu::GpuAdapter>) -> wasmtime::Result<()> {
        self.table.delete(adapter).unwrap();
        Ok(())
    }
}

#[async_trait::async_trait]
impl webgpu::HostGpuDeviceQueue for HostState {
    async fn submit(
        &mut self,
        daq: Resource<DeviceAndQueue>,
        val: Vec<Resource<webgpu::GpuCommandBuffer>>,
    ) -> wasmtime::Result<()> {
        let command_buffers = val
            .into_iter()
            .map(|buffer| self.table.delete(buffer).unwrap())
            .collect::<Vec<_>>();

        let daq = self.table.get(&daq).unwrap();
        daq.queue.submit(command_buffers);

        Ok(())
    }

    fn drop(&mut self, _rep: Resource<webgpu::GpuDeviceQueue>) -> wasmtime::Result<()> {
        // todo!()
        Ok(())
    }
}

pub struct CommandEncoderWithRenderPassRaw {
    // Never None.
    command_encoder: Option<wgpu::CommandEncoder>,
    render_pass: Option<wgpu::RenderPass<'static>>,
    _pin: PhantomPinned,
}

pub struct CommandEncoderWithRenderPass {
    raw: Pin<Box<CommandEncoderWithRenderPassRaw>>,
}

impl CommandEncoderWithRenderPass {
    fn new(command_encoder: wgpu::CommandEncoder) -> Self {
        Self {
            raw: Box::pin(CommandEncoderWithRenderPassRaw {
                command_encoder: Some(command_encoder),
                // render_pass: std::ptr::NonNull::dangling(),
                render_pass: None,
                _pin: PhantomPinned,
            }),
        }
    }

    fn _command_encoder(&self) -> &wgpu::CommandEncoder {
        self.raw.command_encoder.as_ref().unwrap()
    }

    fn command_encoder_mut(&mut self) -> &mut wgpu::CommandEncoder {
        let raw = self._get_raw_mut();
        raw.command_encoder.as_mut().unwrap()
    }

    fn _render_pass<'a>(&'a self) -> Option<&wgpu::RenderPass<'a>> {
        self.raw.render_pass.as_ref()
    }

    // fn render_pass_mut<'a>(&'a mut self) -> Option<&'a mut wgpu::RenderPass<'static>> {
    //     let raw = self._get_raw_mut();
    //     raw.render_pass.as_mut()
    // }

    fn render_pass_mut<'a>(&'a mut self) -> Option<&'a mut wgpu::RenderPass<'a>> {
        let raw = self._get_raw_mut();
        let render_pass = raw.render_pass.as_mut();

        let render_pass: Option<&mut wgpu::RenderPass<'a>> =
            unsafe { std::mem::transmute(render_pass) };

        render_pass
    }

    // fn render_pass_mut<'a>(&'a mut self) -> Option<&'a mut wgpu::RenderPass<'a>> {
    //     // let g: Option<&mut wgpu::RenderPass<'static>> = self.render_pass.as_mut();
    //     // g
    //     // todo!()

    //     let raw = self._get_raw_mut();
    //     raw.render_pass.as_mut()
    // }

    fn begin_render_pass(&mut self, desc: &wgpu::RenderPassDescriptor) {
        let render_pass = self.command_encoder_mut().begin_render_pass(desc);
        let render_pass: wgpu::RenderPass<'static> = unsafe { std::mem::transmute(render_pass) };

        let raw = self._get_raw_mut();
        raw.render_pass = Some(render_pass);
        // mut_host_command_encoder.render_pass = Some(render_pass);
    }

    fn _get_raw_mut<'a>(&'a mut self) -> &'a mut CommandEncoderWithRenderPassRaw {
        unsafe {
            let raw: Pin<&mut CommandEncoderWithRenderPassRaw> = Pin::as_mut(&mut self.raw);
            let raw = Pin::get_unchecked_mut(raw);
            raw
        }
    }

    fn take_render_pass<'a>(&'a mut self) -> Option<wgpu::RenderPass<'a>> {
        let raw = self._get_raw_mut();
        raw.render_pass.take()
    }

    fn take_command_encoder(mut self) -> wgpu::CommandEncoder {
        let raw = self._get_raw_mut();
        raw.command_encoder.take().unwrap()
    }
}

#[async_trait::async_trait]
impl webgpu::HostGpuCommandEncoder for HostState {
    async fn begin_render_pass(
        &mut self,
        cwr: Resource<CommandEncoderWithRenderPass>,
        desc: webgpu::GpuRenderPassDescriptor,
    ) -> wasmtime::Result<Resource<webgpu::GpuRenderPass>> {
        let cwr_rep = cwr.rep();
        let cwr_and_views = self.table.iter_entries({
            let mut m = HashMap::new();
            m.insert(cwr.rep(), 0);
            for (i, color_attachment) in desc.color_attachments.iter().enumerate() {
                m.insert(color_attachment.view.rep(), i + 1);
            }
            m
        });

        let mut cwr: Option<&mut CommandEncoderWithRenderPass> = None;
        let mut views: Vec<Option<&wgpu::TextureView>> = vec![None; desc.color_attachments.len()];

        for (cwr_or_view, rep) in cwr_and_views {
            let cwr_or_view = cwr_or_view.unwrap();

            if rep == 0 {
                let val = cwr_or_view
                    .downcast_mut::<CommandEncoderWithRenderPass>()
                    .unwrap();
                cwr = Some(val);
            } else {
                let val = cwr_or_view.downcast_ref::<wgpu::TextureView>().unwrap();
                views[rep - 1] = Some(val);
            }
        }

        let cwr = cwr.unwrap();
        let views: Vec<&wgpu::TextureView> = views.into_iter().map(|v| v.unwrap()).collect();

        let mut color_attachments = vec![];
        for (i, _color_attachment) in desc.color_attachments.iter().enumerate() {
            color_attachments.push(Some(wgpu::RenderPassColorAttachment {
                view: &views[i],
                resolve_target: None,
                ops: wgpu::Operations {
                    // load: wgpu::LoadOp::Clear(wgpu::Color::BLUE),
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.1,
                        a: 0.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            }));
        }

        cwr.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &color_attachments,
            label: None,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        Ok(Resource::new_own(cwr_rep))
    }

    async fn finish(
        &mut self,
        command_encoder: Resource<CommandEncoderWithRenderPass>,
    ) -> wasmtime::Result<Resource<webgpu::GpuCommandBuffer>> {
        let command_encoder = self.table.delete(command_encoder).unwrap();
        let command_encoder = command_encoder.take_command_encoder();
        let command_buffer = command_encoder.finish();
        Ok(self.table.push(command_buffer).unwrap())
    }

    fn drop(&mut self, _rep: Resource<CommandEncoderWithRenderPass>) -> wasmtime::Result<()> {
        // self.web_gpu_host.encoders.remove(&rep.rep());
        Ok(())
    }
}

#[async_trait::async_trait]
impl webgpu::HostGpuRenderPass for HostState {
    async fn set_pipeline(
        &mut self,
        cwr: Resource<CommandEncoderWithRenderPass>,
        pipeline: Resource<webgpu::GpuRenderPipeline>,
    ) -> wasmtime::Result<()> {
        let cwr_rep = cwr.rep();
        let pipeline_rep = pipeline.rep();
        let cwr_and_pipeline = self.table.iter_entries({
            let mut m = HashMap::new();
            m.insert(cwr_rep, cwr_rep);
            m.insert(pipeline_rep, pipeline_rep);
            m
        });

        let mut cwr: Option<&mut CommandEncoderWithRenderPass> = None;
        let mut pipeline: Option<&webgpu::GpuRenderPipeline> = None;

        for (cwr_or_pipeline, rep) in cwr_and_pipeline {
            let cwr_or_pipeline = cwr_or_pipeline.unwrap();

            if rep == cwr_rep {
                let val = cwr_or_pipeline
                    .downcast_mut::<CommandEncoderWithRenderPass>()
                    .unwrap();
                cwr = Some(val);
            } else if rep == pipeline_rep {
                let val = cwr_or_pipeline
                    .downcast_ref::<webgpu::GpuRenderPipeline>()
                    .unwrap();
                pipeline = Some(val);
            }
        }

        let cwr = cwr.unwrap();
        let pipeline = pipeline.unwrap();

        cwr.render_pass_mut().unwrap().set_pipeline(&pipeline);

        Ok(())
    }

    async fn draw(
        &mut self,
        cwr: Resource<CommandEncoderWithRenderPass>,
        count: u32,
    ) -> wasmtime::Result<()> {
        let cwr = self.table.get_mut(&cwr).unwrap();

        cwr.render_pass_mut().unwrap().draw(0..count, 0..1);

        Ok(())
    }

    async fn end(&mut self, _cwr: Resource<CommandEncoderWithRenderPass>) -> wasmtime::Result<()> {
        todo!()
    }

    fn drop(&mut self, cwr: Resource<CommandEncoderWithRenderPass>) -> wasmtime::Result<()> {
        let cwr = self.table.get_mut(&cwr).unwrap();
        cwr.take_render_pass();
        Ok(())
    }
}

impl From<&wgpu::TextureFormat> for webgpu::GpuTextureFormat {
    fn from(value: &wgpu::TextureFormat) -> Self {
        match value {
            wgpu::TextureFormat::Bgra8UnormSrgb => webgpu::GpuTextureFormat::Bgra8UnormSrgb,
            _ => todo!(),
        }
    }
}
impl From<&webgpu::GpuTextureFormat> for wgpu::TextureFormat {
    fn from(value: &webgpu::GpuTextureFormat) -> Self {
        match value {
            webgpu::GpuTextureFormat::Bgra8UnormSrgb => wgpu::TextureFormat::Bgra8UnormSrgb,
        }
    }
}

impl From<&webgpu::GpuPrimitiveTopology> for wgpu::PrimitiveTopology {
    fn from(value: &webgpu::GpuPrimitiveTopology) -> Self {
        match value {
            webgpu::GpuPrimitiveTopology::PointList => wgpu::PrimitiveTopology::PointList,
            webgpu::GpuPrimitiveTopology::LineList => wgpu::PrimitiveTopology::LineList,
            webgpu::GpuPrimitiveTopology::LineStrip => wgpu::PrimitiveTopology::LineStrip,
            webgpu::GpuPrimitiveTopology::TriangleList => wgpu::PrimitiveTopology::TriangleList,
            webgpu::GpuPrimitiveTopology::TriangleStrip => wgpu::PrimitiveTopology::TriangleStrip,
        }
    }
}
impl From<&wgpu::PrimitiveTopology> for webgpu::GpuPrimitiveTopology {
    fn from(value: &wgpu::PrimitiveTopology) -> Self {
        match value {
            wgpu::PrimitiveTopology::PointList => webgpu::GpuPrimitiveTopology::PointList,
            wgpu::PrimitiveTopology::LineList => webgpu::GpuPrimitiveTopology::LineList,
            wgpu::PrimitiveTopology::LineStrip => webgpu::GpuPrimitiveTopology::LineStrip,
            wgpu::PrimitiveTopology::TriangleList => webgpu::GpuPrimitiveTopology::TriangleList,
            wgpu::PrimitiveTopology::TriangleStrip => webgpu::GpuPrimitiveTopology::TriangleStrip,
        }
    }
}
