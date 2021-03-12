#version 430 core

in vec2 f_uv;
in vec4 f_color;

out vec4 frag_color;

uniform sampler2D font_atlas;

void main() {
    frag_color = f_color * texture(font_atlas, f_uv).r;
}