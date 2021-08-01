#version 430 core

layout (location = 0) in vec3 position;
layout (location = 1) in vec3 normal;
layout (location = 2) in vec4 color;
layout (location = 3) in mat4 model_matrix;

out vec4 f_pos;
out vec4 f_color;
out vec3 f_normal;
out vec3 view_direction;
flat out int instance_id;

uniform mat4 view_projection;

void main() {
    vec4 world_space_pos = model_matrix * vec4(position, 1.0);

    f_pos = world_space_pos;
    f_normal = normal;
    f_color = color;
    instance_id = gl_InstanceID;
    gl_Position = view_projection * world_space_pos;
}