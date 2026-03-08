# xrizer render models

xrizer ships with render models to help identify where the controller sits in 3D space. The models are heavily optimized and meant to only represent the silhouette of the device, without detailed features.

The models here are derivative works of models from the MIT-licensed [webxr-input-proviles/assets](https://github.com/immersive-web/webxr-input-profiles/tree/main/packages/assets) package.

Our meshes retain the bare minimum geometry data that is required to reconstruct the silhouette of the device; vertices, edges, faces

The following data is NOT included:
- Physicial features such as buttons, joysticks, triggers, or metadata about how these actuate
- Textures that may or may not contain registered trademarks
- Vertex UV data, vertex color data, vertex normals, face normals
