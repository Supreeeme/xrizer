#version 450
layout(set = 0, binding = 0) uniform sampler2D overlay;
layout(location = 0) in vec2 texCoord;
layout(push_constant, std430) uniform pc {
	layout(offset = 16) float alpha;
};
layout(location = 0) out vec4 color;

void main() {
	color = texture(overlay, texCoord);
	color.a *= alpha;
}
