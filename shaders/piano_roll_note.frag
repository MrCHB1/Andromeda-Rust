#version 330

out vec4 fragColor;

in vec2 uv;
in vec3 color;

in float noteWidth;
in float noteHeight;

uniform float width;
uniform float height;

void main() {
    float borders = 1.0;
    
    if (uv.x * noteWidth <= 1.0 / width || (1.0 - uv.x) * noteWidth <= 0.5 / width) {
        borders = 0.1;
    }

    if (uv.y * noteHeight <= 0.5 / height || (1.0 - uv.y) * noteHeight <= 0.5 / height) {
        borders = 0.1;
    }

    fragColor = vec4(color * borders, 1.0);
}