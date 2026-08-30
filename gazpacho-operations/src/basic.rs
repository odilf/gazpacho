use gazpacho_datatypes::{Extent, Fps, Frame, Resolution};

use crate::{Operation, Renderer, Request};

crate::op! {
    pub struct Load { path } as "load"
}

impl Operation for Load {
    fn frame(&self, renderer: &mut impl Renderer, req: Request) -> eyre::Result<Frame> {
        let path = renderer.eval(self.path(), req)?.to_str()?;
        renderer.load_frame(path, req)
    }

    fn extent(&self, renderer: &mut impl Renderer) -> eyre::Result<Extent> {
        let path = renderer.eval(self.path(), Request::sentinel())?.to_str()?;
        renderer.load_extent(path)
    }

    fn resolution(&self, renderer: &mut impl Renderer) -> eyre::Result<Resolution> {
        let path = renderer.eval(self.path(), Request::sentinel())?.to_str()?;
        renderer.load_resolution(path)
    }

    fn fps(&self, renderer: &mut impl Renderer) -> eyre::Result<Option<Fps>> {
        let path = renderer.eval(self.path(), Request::sentinel())?.to_str()?;
        renderer.load_fps(path).map(Some)
    }
}

crate::op! {
    pub struct Concat { a, b } as "concat"
}

impl Operation for Concat {
    fn frame(&self, renderer: &mut impl Renderer, req: Request) -> eyre::Result<Frame> {
        let ext_a = renderer.extent(self.a())?;
        let val = if ext_a.contains(&req.time) {
            renderer.eval(self.a(), req)
        } else {
            renderer.eval(
                self.b(),
                Request {
                    time: req.time - ext_a.duration(),
                    ..req
                },
            )
        }?;

        val.to_frame()
    }

    fn extent(&self, renderer: &mut impl Renderer) -> eyre::Result<Extent> {
        let ext_a = renderer.extent(self.a())?;
        let ext_b = renderer.extent(self.b())?;
        #[expect(clippy::unwrap_used, reason = "ext_a.end + duration >= ext_a.end")]
        Ok(Extent::new(ext_a.start, ext_a.end + ext_b.duration()).unwrap())
    }

    fn resolution(&self, renderer: &mut impl Renderer) -> eyre::Result<Resolution> {
        let res_a = renderer.resolution(self.a())?;
        let res_b = renderer.resolution(self.b())?;
        if res_a == res_b {
            Ok(res_a)
        } else {
            eyre::bail!("Concating two different resolutions is unsupported.")
        }
    }

    fn fps(&self, renderer: &mut impl Renderer) -> eyre::Result<Option<Fps>> {
        let fps_a = renderer.fps(self.a())?;
        let fps_b = renderer.fps(self.b())?;
        if fps_a == fps_b {
            Ok(fps_a)
        } else {
            eyre::bail!("Concating videos with different FPS is unsupported.")
        }
    }
}
