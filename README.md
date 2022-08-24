# Procreate WGSL File Renderer

Takes a Procreate file and composits its layers together. This produces slightly
different results from the reference render by Procreate's engine.

This renders the file using the GPU, leveraging the `wgpu` crate.

## Note
The compositor produces accurate blending results except for:
* Hard Light
* Overlay
* Saturation
* Hue

## Procreate File Format
All `.procreate` files are actually standard ZIP files.
```
- (Layer folders named by their UUID)
  - Contains .chunk files, presumably the actual pixel canvas data for the document.
- QuickLook [Folder]
  - Thumbnail.png - Low-quality screenshot generated by Procreate.
- video [Folder]
  - segments [Folder]
    - segment-X.mp4, where X is a number starting from 1.
- Document.archive - NSKeyedArchive containing layer information along with other document information like canvas size.
```

## Raster Canvas Data
Each layer in a Procreate file has a `uuid` associated with it. It's raw RGBA data is located
under `{uuid}/`. The folder contain chunks with the naming convention `{col}~{row}.chunk`,
which are `tile_size * tile_size` raw RGBA data that has been compressed with LZO.
Recombine these chunks together to obtain the raw layer data.
* It is important to note that the raw layer data is **premultiplied** RGBA.

## Attribution
* [https://git.sr.ht/~redstrate/silica-viewer] Base code for understanding the Procreate format.