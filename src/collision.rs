pub struct LineSegment {
    pub p0: glm::TVec4<f32>,
    pub p1: glm::TVec4<f32>
}

//An infinite plane as defined by a point on the plane and a vector normal to the plane
pub struct Plane {
    pub point: glm::TVec4<f32>,
    pub normal: glm::TVec4<f32>,
}

impl Plane {
    pub fn new(point: glm::TVec4<f32>, normal: glm::TVec4<f32>) -> Self {
        Plane {
            point,
            normal
        }
    }
}

pub struct PlaneBoundaries {
    pub xmin: f32,
    pub xmax: f32,
    pub ymin: f32,
    pub ymax: f32
}

//Axis-aligned bounding box
pub struct AABB {
    pub position: glm::TVec4<f32>,
    pub width: f32,
    pub depth: f32,
    pub height: f32
}

pub fn segment_intersect_plane(plane: &Plane, segment: &LineSegment) -> Option<glm::TVec4<f32>> {
    let denominator = glm::dot(&plane.normal, &(segment.p1 - segment.p0));

    //Check for divide-by-zero
    if denominator != 0.0 {
        let x = glm::dot(&plane.normal, &(plane.point - segment.p0)) / denominator;
        if x > 0.0 && x <= 1.0 {
            let result = (1.0 - x) * segment.p0 + x * segment.p1;
            Some(glm::vec4(result.x, result.y, result.z, 1.0))
        } else {
            None
        }        
    } else {
        None
    }
}

pub fn standing_on_plane(plane: &Plane, segment: &LineSegment, boundaries: &PlaneBoundaries) -> Option<glm::TVec4<f32>> {
    let collision_point = segment_intersect_plane(&plane, &segment);
    if let Some(point) = collision_point {
        let on_aabb = point.x > boundaries.xmin &&
                      point.x < boundaries.xmax &&
                      point.y > boundaries.ymin &&
                      point.y < boundaries.ymax;

        if on_aabb {
            Some(point)
        } else {
            None
        }
    } else {
        None
    }
}

pub fn point_plane_distance(point: &glm::TVec4<f32>, plane: &Plane) -> f32 {
    glm::dot(&plane.normal, &(point - plane.point))
}