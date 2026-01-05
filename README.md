# jamon

Good video editing software for all.

Principles:
-

# How it works

Really, what jamon mostly does is help you build a GPU pipeline to render video. Specifically, it handles:
- Reading and writing video files
- Build a render graph using the GPU
- Previewing and modyfing the render graph using a GUI


Your video is represented as a tree of nodes.

Each node can have some inputs.

A clip is a renderable node. A node can be rendered if it has [`FrameIndex`] as input and a [`Frame`] as output.

TODO: Nodes
- [ ] Concat
- [ ] Sequence
- [ ] Cuts
- [ ] Color grading (contrast, curves, color adjustment, etc.)
- [ ] Transform
- [ ] Overlay
- [ ] Generators
- [ ] Use sidechain to duck music using dialogue audio
- [ ] Auto cut based on audio
