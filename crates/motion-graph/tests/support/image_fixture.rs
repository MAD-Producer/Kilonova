use image::ImageEncoder;
use motion_graph::*;

pub fn png(pixels: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(pixels, width, height, image::ExtendedColorType::Rgba8)
        .unwrap();
    bytes
}
pub fn image() -> GraphImage {
    GraphImage::from_png(png(
        &[
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ],
        2,
        2,
    ))
    .unwrap()
}
pub fn graph(image: GraphImage) -> GraphDocument {
    let mut graph = demo_graph();
    graph.format_version = IMAGE_ASSETS_FORMAT_VERSION;
    graph.id = "image_test".into();
    graph.public_parameters.clear();
    graph.images = [(image.id().to_owned(), image.clone())].into();
    graph.nodes = vec![
        Node::new(
            "image",
            NodeOp::ImageAsset {
                image: image.id().into(),
            },
        ),
        Node::new("uv", NodeOp::Uv),
        Node::new(
            "sample",
            NodeOp::SampleImage {
                coordinates: SampleCoordinates::Uv,
            },
        ),
        Node::new("output", NodeOp::ColorOutput),
    ];
    graph.connections = vec![
        Connection::new("image", "image", "sample", "image"),
        Connection::new("uv", "value", "sample", "coordinate"),
        Connection::new("sample", "color", "output", "color"),
    ];
    graph
}
pub fn many_images(count: usize) -> GraphDocument {
    let mut graph = graph(image());
    graph
        .nodes
        .retain(|node| node.id == "uv" || node.id == "output");
    graph.connections.clear();
    graph.images.clear();
    let mut previous = String::new();
    for index in 0..count {
        let image = GraphImage::from_png(png(&[index as u8, 100, 200, 255], 1, 1)).unwrap();
        let id = image.id().to_owned();
        graph.images.insert(id.clone(), image);
        let input = format!("image_{index}");
        let sample = format!("sample_{index}");
        graph
            .nodes
            .push(Node::new(&input, NodeOp::ImageAsset { image: id }));
        graph.nodes.push(Node::new(
            &sample,
            NodeOp::SampleImage {
                coordinates: SampleCoordinates::Uv,
            },
        ));
        graph.connections.extend([
            Connection::new(&input, "image", &sample, "image"),
            Connection::new("uv", "value", &sample, "coordinate"),
        ]);
        if index == 0 {
            previous = sample;
        } else {
            let blend = format!("blend_{index}");
            let mut node = Node::new(
                &blend,
                NodeOp::Blend {
                    mode: ColorBlendMode::Normal,
                },
            );
            node.input_defaults = authoring_input_defaults(&node.operation);
            graph.nodes.push(node);
            graph.connections.extend([
                Connection::new(&previous, "color", &blend, "backdrop"),
                Connection::new(&sample, "color", &blend, "source"),
            ]);
            previous = blend;
        }
    }
    graph
        .connections
        .push(Connection::new(&previous, "color", "output", "color"));
    graph
}
