#version 450
layout(set = 0, binding = 0) uniform sampler2D overlay;
layout(location = 0) in vec2 texCoord;
layout(location = 0) out vec4 color;

void main() {
	color = texture(overlay, texCoord);
}
