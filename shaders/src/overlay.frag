#version 450
layout(set = 0, binding = 0) uniform sampler2D overlay;
layout(location = 0) in vec2 texCoord;
layout(location = 0) out vec4 color;

// Set when the source view is UNORM but the target is _SRGB, so the write
// would encode already-encoded values a second time.
layout(constant_id = 0) const bool decode_srgb = false;

vec3 srgb_to_linear(vec3 c)
{
	bvec3 low = lessThanEqual(c, vec3(0.04045));
	return mix(pow((c + 0.055) / 1.055, vec3(2.4)), c / 12.92, low);
}

void main() {
	color = texture(overlay, texCoord);
	if (decode_srgb)
		color.rgb = srgb_to_linear(color.rgb);
}
