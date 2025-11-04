This crate generates Rust bindings from the various versioned OpenVR headers using bindgen.

## Process

The build script does three major things:

1. Create bindgen bindings from each of the provided headers
2. Parse the bindings, unifying the structs that are the same or otherwise compatible
from across the different headers and generating traits for each version of the versioned interfaces
(IVRSystem, IVRCompositor, etc)
3. Combine all of the parsed bindings into a file to be consumed by xrizer

## Bindings parsing

For the unversioned structs, the ones that add members are typically accessed from behind a pointer,
so we can simply treat newer versions the same as older versions and take care not to access the wrong members.
This is done in the OpenVR API through passing the struct size when it's used. `Compositor_FrameTiming` is one
example of such a struct.

- Sometimes there are breaking changes between structs and header versions, so these incompatible structs are
put into their own submodule. An example is `vr_0_9_12::Compositor_FrameTiming`, generated because there were
members of the struct changed in a backwards incompatible way later.

For the versioned interfaces, a trait is generated for each version, such as `IVRSystem022_Interface`. When the only
change between an older version and newer version of an interface is a method is added, implementing the newer interface
will automatically implement the older interface too. `IVRCompositor_027` to `IVRCompositor_028` is an example of this.
However, for some interfaces methods are either removed or changed, such as with `IVRSystem021` to `IVRSystem022`.
In this case, a trait will be generated that has only the removed or changed methods, such as `IVRSystem021On022`. Once
this trait and the newer version trait is implemented, the older version will be implemented too.
