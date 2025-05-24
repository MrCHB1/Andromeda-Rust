#version 330

out vec4 fragColor;

in vec2 uv;
in float oddBarFac;
in float bLength;

uniform float width;
uniform float height;

void main() {
    float key_pos = uv.y * 128.0;
    int key_int = int(key_pos) % 12;
    float key_sharp_fac = (key_int == 1 || key_int == 3 || key_int == 6 || key_int == 8 || key_int == 10) ? 0.7 : 1.0;

    float beat_pos = uv.x * 4.0;
    int beat_int = int(beat_pos) % 2;
    float beat_odds_fac = (beat_int == 0) ? 0.9 : 1.0;

    vec3 color = vec3(0.2, 0.2, 0.25);
    color *= key_sharp_fac;
    color *= beat_odds_fac;
    color *= oddBarFac;
    if (uv.x * bLength <= 1.5 / width) {
        color *= 0.1;
    }
    if (fract(beat_pos) * (bLength / 4.0) <= 0.5 / width) {
        color *= 0.1;
    }
    if (fract(key_pos) <= 0.07) {
        color *= 0.3;
    }
    fragColor = vec4(color, 1.0);
}