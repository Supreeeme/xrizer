FROM ubuntu:22.04

ARG DEBIAN_FRONTEND=noninteractive
ENV TZ=Etc/UTC
RUN apt-get update \
	&& apt-get install -y cmake ninja-build clang libvulkan-dev libx11-xcb-dev curl wget git python3 \
	&& rm -r /var/lib/apt/lists/*
RUN git clone https://github.com/google/shaderc --depth 1 -b v2024.4
RUN cd shaderc \
	&& ./utils/git-sync-deps \
	&& cmake -GNinja -Bbuild -DCMAKE_BUILD_TYPE=Release \
          -DSHADERC_SKIP_TESTS=ON \
          -DSHADERC_SKIP_EXAMPLES=ON \
          -DSHADERC_SKIP_COPYRIGHT_CHECK=ON \
          -DSHADERC_ENABLE_WERROR_COMPILE=OFF . \
	&& ninja -C build install \
	&& which glslc
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
	&& . "$HOME/.cargo/env" \
	&& rustup toolchain install stable \
	&& rustup toolchain install nightly \
	&& rustup +nightly component add miri
