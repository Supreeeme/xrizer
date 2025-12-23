# xrizer - XR-ize your OpenVR games

xrizer is a reimplementation of OpenVR on top of OpenXR. This enables you to run OpenVR games through any OpenXR runtime without running SteamVR.

Note that xrizer is currently immature. Many things are likely broken, so please open bugs! For a more mature solution, check out [OpenComposite](https://gitlab.com/znixian/OpenComposite), which some of the code in this repo is based on.

# Download & Usage

[Latest nightly](https://nightly.link/Supreeeme/xrizer/workflows/ci/main/xrizer-nightly-release.zip) - Bleeding edge!

[Latest release](https://github.com/Supreeeme/xrizer/releases/latest/download/xrizer-release.zip)

Choose a download and click it to download a zip. Unzip it and place it wherever you like.

In order to use xrizer, you must change where OpenVR games search for the runtime. There are two ways to accomplish this:

1. Add the path to xrizer to `$XDG_CONFIG_HOME/openvr/openvrpaths.vrpath`.
2. Set the `VR_OVERRIDE` environment variable when launching a game: `VR_OVERRIDE=/path/to/xrizer <command>` (steam launch options: `VR_OVERRIDE=/path/to/xrizer %command%`)
   - Note that if you do this method, you must have an existing valid `openvrpaths.vrpath` file, though xrizer doesn't need to be in it.

## openvrpaths.vrpath

This file is a JSON file read by OpenVR games to determine where the runtime is located. If you have SteamVR installed and have run it before, you can simply add the path to xrizer to the "runtime" section.
If you haven't, here is a sample file that can be placed in `$XDG_CONFIG_HOME/openvr/openvrpaths.vrpath` (be sure to update the path!):
```json
{
    "version": 1,
    "runtime": [
        "/path/to/xrizer"
    ]
}
```

## Steam Linux Runtime

When running games through the Steam Linux Runtime - which is all of them when using Proton 6 or newer - filesystem paths will not work the same way since it's a container. Some things to keep in mind are:
- Any path in your home directory will always be available
- Paths under `/usr` will be available at `/run/host/usr`
- Some paths will be completely unavailable.

You can make paths available within the container using the `PRESSURE_VESSEL_FILESYSTEMS_RW` env var. An example of what you might put in your game's steam launch option if you want to use xrizer with Monado installed systemwide is:
```
XR_RUNTIME_JSON=/run/host/usr/share/openxr/1/openxr_monado.json VR_OVERRIDE=/path/to/xrizer PRESSURE_VESSEL_FILESYSTEMS_RW=$XDG_RUNTIME_DIR/monado_comp_ipc
```
For more info on the container, see [Valve's docs on Pressure Vessel](https://gitlab.steamos.cloud/steamrt/steam-runtime-tools/-/blob/main/pressure-vessel/wrap.1.md).

# FAQ

## What games work on xrizer?

You tell me! The aim is for all standard (non overlay/utility/background) OpenVR apps to function as they would on SteamVR. Obviously this is not 100% the case, so open issues as you run into games that don't work properly and they will be addressed in time.

## Why rewrite OpenComposite?

OpenComposite has several years of existence over xrizer, so rewriting it is no small task. However, OpenComposite also lacks sufficient testing infrastructure, making it easy to inadvertently introduce regressions, and the way it's architected makes it difficult to write simple tests. OpenComposite was also not originally designed to utilize OpenXR, and there's still some legacy stuff from that period remaining in the codebase, which can make it more convoluted to understand. Dealing with these issues for a while led me to conclude that it would be more productive to rewrite it.

## Why Rust?

I like Rust. I don't like CMake.

# Building
```
# dev build
cargo xbuild

# release build
cargo xbuild --release
```

After building, the output directory can be used as a runtime directory. If you built the dev build, this will be `<path to xrizer repo>/target/debug`, and for the release build this is `<path to xrizer repo>/target/release`.

# Contributing

All contributions welcome.
- If you're opening a bug, please submit a log. The log is located at `$XDG_STATE_HOME/xrizer/xrizer.txt`, or `$HOME/.local/state/xrizer/xrizer.txt` if `$XDG_STATE_HOME` is not set.
- If submitting pull requests, please consider writing a test if possible/helpful - OpenVR is a large API surface and games are fickle, so ensuring things are well tested prevents future unintentional breakage.

# Environment Variables
_RUST_LOG_ - This is used for adjusting the logging of xrizer. See the [env_logger documentation](https://docs.rs/env_logger/latest/env_logger/#enabling-logging) for understanding how this works. Here are some useful nonstandard logging targets:
- `openvr_calls` - logs the name of each OpenVR function as they are called
- `tracked_property` - logs the name and device index of each requested tracked device property.

_XRIZER_CUSTOM_BINDINGS_DIR_ - This can be used to supply a directory that xrizer will search for controller bindings files. Note that the format of these bindings aren't actually documented anywhere, but it's easy enough to modify an existing file, and xrizer parses them so you can read the source too.

_XRIZER_TRACKER_SERIALS_ - This is a semi-colon (`;`) separated list of device serial numbers to use as generic trackers. Can be used to assign controllers as FBT trackers.

# See also

- [OpenComposite](https://gitlab.com/znixian/OpenOVR) - The original OpenVR/OpenXR implementation, much more mature than xrizer. Some of the code in this repo was rewritten based on OpenComposite.
- [OpenVR](https://github.com/ValveSoftware/openvr)
- [Monado](https://gitlab.freedesktop.org/monado/monado) - An open source OpenXR runtime
- [WiVRn](https://github.com/WiVRn/WiVRn) - An OpenXR streaming runtime for standalone headsets, based on Monado.
