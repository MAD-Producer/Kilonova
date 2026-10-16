# Workspace presentation regression

The reported failure was a stale layout after selecting Nodes / Drawing / All,
followed by a black client area when moving the window. This change repairs GPU
presentation only; it does not change panel layout or styling.

## Reproduced failures

On Windows / NVIDIA Vulkan, a fresh native particle fixture presented Default,
then failed on the first Nodes presentation:

```text
RenderPass uses textures with formats [Some(Bgra8UnormSrgb)]
RenderPipeline with 'independent node shader' label uses attachments
with formats [Some(Rgba8UnormSrgb)]
```

The node overlay received the composition scratch format instead of the actual
surface format. The renderer retained this validation failure, so subsequent
workspace presentations also failed. Screenshot capture alone missed the error
because its output texture is RGBA.

After fixing the format, native acceptance exposed a second validation error:

```text
Viewport has invalid rect
{ x: -3.0517578e-5, y: 0.4375, w: 994.00006, h: 559.125 }
render target (994, 560, 1)
```

Aspect fitting with single precision could overshoot an attachment by a fraction
of a pixel. Composition passes, compositing uniforms, and retained presentation
now share a double precision fit whose final dimensions stay within the viewport.

## Regression coverage

- `viewport::tests`: landscape, portrait, odd widths from 1 through 2048 and
  small/large heights remain within the target; center and aspect fit are checked.
- Native desktop acceptance now switches through all five presets and back,
  presenting each twice on the real surface with an installed live node preview.
  It fails on GPU errors, missing presentations, unintended edit commands, or
  changes to the project. The report includes `workspacePresentationProbe`.
- `tools/ci_desktop.py` requires the native probe in both language runs, alongside
  its existing floating window, node, screenshot, style, and shared MCP checks.

The first fixed Windows debug run presented both frames for each preset in
26.5–58.8 ms. This is a local regression measurement, not a cross-device latency
guarantee. No screenshot of the user's desktop or private project is committed.
There are no new dependencies or third-party assets. This focused repair does
not close the broader threading/lifecycle audit in issue #130.
