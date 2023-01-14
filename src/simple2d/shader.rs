use web_sys::WebGlShader;
use web_sys::WebGlUniformLocation;
use web_sys::{WebGl2RenderingContext, WebGlProgram};

use super::IndexBuffer;
use super::TextureBuffer;
use super::TextureCoordBuffer;

///
/// A webgl2 buffer that automatically deletes itself when dropped.
///
pub struct Buffer {
    pub(crate) buffer: web_sys::WebGlBuffer,
    pub(crate) num_verts: usize,
    pub(crate) ctx: WebGl2RenderingContext,
}
impl Buffer {
    pub fn new(ctx: &WebGl2RenderingContext) -> Result<Self, String> {
        let buffer = ctx.create_buffer().ok_or("failed to create buffer")?;
        Ok(Buffer {
            buffer,
            num_verts: 0,
            ctx: ctx.clone(),
        })
    }
}
impl Drop for Buffer {
    fn drop(&mut self) {
        self.ctx.delete_buffer(Some(&self.buffer));
    }
}

impl GlProgram {
    pub fn draw(
        &self,
        texture:&TextureBuffer,
        texture_coords:&TextureCoordBuffer,
        indexes:Option<&IndexBuffer>,
        buffer: &Buffer,
        primitive: u32,
        mmatrix: &[f32; 16],
        point_size: f32,
        normals:&Buffer,
        world_inverse_transpose:&[f32;16]
    ) {
        if buffer.num_verts == 0 {
            return;
        }

        let context = &buffer.ctx;

        context.use_program(Some(&self.program));

        
        context.uniform1f(Some(&self.point_size), point_size);
        // context.uniform4fv_with_f32_array(Some(&self.bg), color);
        //context.enable_vertex_attrib_array(texture_coords.0.buffer);
        // We'll supply texcoords as floats.

        context.enable_vertex_attrib_array(self.texcoord);
        context.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&texture_coords.0.buffer));
        context.vertex_attrib_pointer_with_i32(
            self.texcoord as u32,
            2,
            WebGl2RenderingContext::FLOAT,
            false,
            0,
            0,
        ); 
        

        

        context.enable_vertex_attrib_array(self.position);
        context.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&buffer.buffer));

        context.vertex_attrib_pointer_with_i32(
            self.position as u32,
            3,
            WebGl2RenderingContext::FLOAT,
            false,
            0,
            0,
        );


        context.enable_vertex_attrib_array(self.normal);
        context.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&normals.buffer));

        context.vertex_attrib_pointer_with_i32(
            self.normal as u32,
            3,
            WebGl2RenderingContext::FLOAT,
            false,
            0,
            0,
        );


        context.uniform_matrix4fv_with_f32_array(Some(&self.world_inverse_transpose), false, world_inverse_transpose);
        

        context.uniform_matrix4fv_with_f32_array(Some(&self.mmatrix), false, mmatrix);

        context.bind_texture(WebGl2RenderingContext::TEXTURE_2D, Some(&texture.texture));
        
        

        if let Some(indexes)=indexes{
            context.bind_buffer(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, Some(&indexes.0.buffer));
            context.draw_elements_with_i32(primitive, indexes.0.num_verts as i32,WebGl2RenderingContext::UNSIGNED_SHORT,0)
        }else{
            context.draw_arrays(primitive, 0, buffer.num_verts as i32)
        }
    }

    pub fn new(context: &WebGl2RenderingContext, vs: &str, fs: &str) -> Result<Self, String> {
        let vert_shader = compile_shader(context, WebGl2RenderingContext::VERTEX_SHADER, vs)?;
        let frag_shader = compile_shader(context, WebGl2RenderingContext::FRAGMENT_SHADER, fs)?;
        let program = link_program(context, &vert_shader, &frag_shader)?;

        context.delete_shader(Some(&vert_shader));
        context.delete_shader(Some(&frag_shader));

        let mmatrix = context
            .get_uniform_location(&program, "mmatrix")
            .ok_or_else(|| "uniform err".to_string())?;


        let point_size = context
            .get_uniform_location(&program, "point_size")
            .ok_or_else(|| "uniform err".to_string())?;

        // let bg = context
        //     .get_uniform_location(&program, "bg")
        //     .ok_or_else(|| "uniform err".to_string())?;
        let position = context.get_attrib_location(&program, "position");

        let normal = context.get_attrib_location(&program, "v_normal");


        let texcoord = context.get_attrib_location(&program, "a_texcoord");

        if position < 0 {
            return Err("attribute err".to_string());
        }

        let world_inverse_transpose = context
        .get_uniform_location(&program, "u_worldInverseTranspose")
        .ok_or_else(|| "uniform err".to_string())?;

        

        let position = position as u32;
        let normal=normal as u32;
        let texcoord=texcoord as u32;
        Ok(GlProgram {
            world_inverse_transpose,
            program,
            mmatrix,
            point_size,
            normal,
            //bg,
            position,
            texcoord
        })
    }
}

pub struct GlProgram {
    pub(crate) program: WebGlProgram,
    mmatrix: WebGlUniformLocation,
    point_size: WebGlUniformLocation,
    world_inverse_transpose:WebGlUniformLocation,
    //bg: WebGlUniformLocation,
    position: u32,
    texcoord:u32,
    normal:u32,
}

fn compile_shader(
    context: &WebGl2RenderingContext,
    shader_type: u32,
    source: &str,
) -> Result<WebGlShader, String> {
    let shader = context
        .create_shader(shader_type)
        .ok_or_else(|| String::from("Unable to create shader object"))?;
    context.shader_source(&shader, source);
    context.compile_shader(&shader);

    if context
        .get_shader_parameter(&shader, WebGl2RenderingContext::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(shader)
    } else {
        Err(context
            .get_shader_info_log(&shader)
            .unwrap_or_else(|| String::from("Unknown error creating shader")))
    }
}

fn link_program(
    context: &WebGl2RenderingContext,
    vert_shader: &WebGlShader,
    frag_shader: &WebGlShader,
) -> Result<WebGlProgram, String> {
    let program = context
        .create_program()
        .ok_or_else(|| String::from("Unable to create shader object"))?;

    context.attach_shader(&program, vert_shader);
    context.attach_shader(&program, frag_shader);
    context.link_program(&program);

    if context
        .get_program_parameter(&program, WebGl2RenderingContext::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(program)
    } else {
        Err(context
            .get_program_info_log(&program)
            .unwrap_or_else(|| String::from("Unknown error creating program object")))
    }
}
