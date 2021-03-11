#version 430 core

in vec2 f_uv;
in vec4 f_color;

out vec4 frag_color;

uniform sampler2D font_atlas;

void main() {
    //float atlas_sample = texture(font_atlas, f_uv).r;
    //frag_color = texture(font_atlas, f_uv);
    //frag_color = f_color + texture(font_atlas, uv);
    //frag_color = vec4(f_uv, 0.0, 1.0);
    //frag_color = f_color * atlas_sample;
    frag_color = f_color * texture(font_atlas, f_uv).r;
}