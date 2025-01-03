FROM ubuntu:22.04

ARG DEBIAN_FRONTEND=noninteractive
ENV TZ=Etc/UTC
RUN apt-get update \
	&& apt-get install -y cmake ninja-build clang libvulkan-dev libx11-xcb-dev curl wget git python3 \
	&& rm -r /var/lib/apt/lists/*
RUN wget -qO- https://packages.lunarg.com/lunarg-signing-key-pub.asc | tee /etc/apt/trusted.gpg.d/lunarg.asc \
	&& wget -qO /etc/apt/sources.list.d/lunarg-vulkan-jammy.list https://packages.lunarg.com/vulkan/lunarg-vulkan-jammy.list \
	&& apt-get update \
	&& apt-get install -y vulkan-sdk \
	&& which glslc
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
	&& . "$HOME/.cargo/env" \
	&& rustup toolchain install stable \
	&& rustup default stable \
	&& rustup toolchain install nightly \
	&& rustup +nightly component add miri
ENV PATH="/root/.cargo/bin:$PATH"
