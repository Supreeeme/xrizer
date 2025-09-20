FROM ubuntu:22.04

ARG DEBIAN_FRONTEND=noninteractive
ENV TZ=Etc/UTC
ENV MIRI_SYSROOT=/miri-sysroot CARGO_HOME=/cargo RUSTUP_HOME=/rustup
RUN apt-get update \
	&& apt-get install -y cmake ninja-build clang libvulkan-dev libopenxr-dev libx11-xcb-dev curl wget git python3 \
	&& rm -r /var/lib/apt/lists/*
RUN wget -qO- https://packages.lunarg.com/lunarg-signing-key-pub.asc | tee /etc/apt/trusted.gpg.d/lunarg.asc \
	&& wget -qO /etc/apt/sources.list.d/lunarg-vulkan-jammy.list https://packages.lunarg.com/vulkan/lunarg-vulkan-jammy.list \
	&& apt-get update \
	&& apt-get install -y shaderc \
	&& which glslc
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
	&& . "/cargo/env" \
	&& rustup toolchain install stable \
	&& rustup default stable \
	&& rustup toolchain install nightly \
	&& rustup +nightly component add miri \
	&& cargo +nightly miri setup
ENV PATH="/cargo/bin:$PATH"
