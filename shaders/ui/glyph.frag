#version 330 core

in vec2 f_uvs;
in vec4 f_color;

out vec4 frag_color;

uniform sampler2D glyph_texture;

void main() {
    frag_color = f_color * texture(glyph_texture, f_uvs).r;
}