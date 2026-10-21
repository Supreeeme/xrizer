#version 450
layout(set = 0, binding = 0) uniform sampler2D overlay;
layout(location = 0) in vec2 texCoord;
layout(location = 0) out vec4 color;

// Manually decode sRGB. The game texture is UNORM but its bytes are
// sRGB-encoded (typical for game backbuffers). Sampling a UNORM view returns
// the raw byte value which the hardware treats as linear. We convert here so
// the sRGB swapchain attachment re-encodes it correctly, giving a lossless
// round-trip through the compositor.
vec3 srgb_to_linear(vec3 c) {
    bvec3 cutoff = lessThan(c, vec3(0.04045));
    vec3 lo = c / vec3(12.92);
    vec3 hi = pow((c + vec3(0.055)) / vec3(1.055), vec3(2.4));
    return mix(hi, lo, cutoff);
}

void main() {
    vec4 sampled = texture(overlay, texCoord);
    color = vec4(srgb_to_linear(sampled.rgb), sampled.a);
}
