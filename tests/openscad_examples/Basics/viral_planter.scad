// Viral Geometric Desk Planter
// Dodecahedron shape with faceted surfaces
// unit: mm

// --- Parameters ---
body_radius = 55;
wall_thickness = 3;
rim_width = 50;       // rim outer radius = 50mm -> diameter ~100mm
rim_height = 4;
top_cut = 20;         // Z height to cut top
bottom_cut = 14;      // Z height below center to cut bottom
drain_hole_r = 3;
drain_hole_count = 4;
drain_hole_spread = 15;
tray_height = 6;
tray_wall = 2;
tray_clearance = 1;
chamfer_height = 5;
$fn = 48;

// --- Robust Dodecahedron ---
// Built by hulling pairs of opposite pentagons

module pentagon(r) {
    polygon([for (i = [0:4]) [r * cos(72*i + 90), r * sin(72*i + 90)]]);
}

module dodecahedron(r) {
    phi = (1 + sqrt(5)) / 2;
    // Scale factor so circumradius = r
    s = r / sqrt(3);
    // Use minkowski-free approach: scaled icosidodecahedron approximation
    // Actually use hull of 20 vertices
    c = s;
    p = s * phi;
    ip = s / phi;
    pts = [
        [ c,  c,  c], [ c,  c, -c], [ c, -c,  c], [ c, -c, -c],
        [-c,  c,  c], [-c,  c, -c], [-c, -c,  c], [-c, -c, -c],
        [0,  ip,  p], [0,  ip, -p], [0, -ip,  p], [0, -ip, -p],
        [ ip,  p, 0], [ ip, -p, 0], [-ip,  p, 0], [-ip, -p, 0],
        [ p, 0,  ip], [ p, 0, -ip], [-p, 0,  ip], [-p, 0, -ip]
    ];
    hull() {
        for (pt = pts) {
            translate(pt) sphere(r = 0.01, $fn = 6);
        }
    }
}

// Faceted dodecahedron using intersection of 12 half-spaces
module faceted_dodecahedron(r) {
    phi = (1 + sqrt(5)) / 2;
    // 12 face normals of dodecahedron (directions of icosahedron vertices)
    dirs = [
        [0,  1,  phi], [0,  1, -phi], [0, -1,  phi], [0, -1, -phi],
        [ 1,  phi, 0], [ 1, -phi, 0], [-1,  phi, 0], [-1, -phi, 0],
        [ phi, 0,  1], [ phi, 0, -1], [-phi, 0,  1], [-phi, 0, -1]
    ];
    d = r * phi / sqrt(1 + phi * phi);   // face distance from center
    big = 500;

    intersection() {
        for (dir = dirs) {
            len = norm(dir);
            nx = dir[0]/len; ny = dir[1]/len; nz = dir[2]/len;
            // Place a half-space: everything on the inside of this face
            // Use multmatrix to orient a cube so its top face has normal = dir
            // Simple approach: use a thick slab
            multmatrix(m = [
                [1 - nx*nx*(1-nz/(nz==0?0.0001:abs(nz)+1)), 0, nx, 0],
                [0, 1, 0, 0],
                [0, 0, 1, 0]
            ])
            // Better: just translate along normal and use huge cube
            translate(d * [nx, ny, nz])
            rotate(acos(min(1, max(-1, nz))) * (norm(cross([0,0,1], [nx,ny,nz])) < 0.001 ? 0 : 1),
                   v = norm(cross([0,0,1], [nx,ny,nz])) < 0.001 ? [1,0,0] : cross([0,0,1], [nx,ny,nz]))
            translate([0, 0, -big])
            cube([2*big, 2*big, 2*big], center = true);
        }
    }
}

// Simpler faceted shape: use low-poly sphere approximation
module geo_body(r) {
    // Use a low-poly sphere to approximate dodecahedron look
    // $fn=5 with rotation gives pentagonal cross-sections
    // Actually, let's just use hull of dodecahedron vertices for the shape
    // and rely on the polyhedron for flat faces
    phi = (1 + sqrt(5)) / 2;
    s = r / sqrt(3);
    c = s;
    p = s * phi;
    ip = s / phi;
    pts = [
        [ c,  c,  c], [ c,  c, -c], [ c, -c,  c], [ c, -c, -c],
        [-c,  c,  c], [-c,  c, -c], [-c, -c,  c], [-c, -c, -c],
        [0,  ip,  p], [0,  ip, -p], [0, -ip,  p], [0, -ip, -p],
        [ ip,  p, 0], [ ip, -p, 0], [-ip,  p, 0], [-ip, -p, 0],
        [ p, 0,  ip], [ p, 0, -ip], [-p, 0,  ip], [-p, 0, -ip]
    ];
    faces = [
        [0, 8, 4, 14, 12],    // top-front-left
        [0, 16, 2, 10, 8],    // top-front-right
        [0, 12, 1, 17, 16],   // right
        [1, 12, 14, 5, 9],    // top-back
        [1, 9, 11, 3, 17],    // back-right
        [2, 16, 17, 3, 13],   // bottom-right
        [2, 13, 15, 6, 10],   // bottom-front
        [4, 8, 10, 6, 18],    // left-front
        [4, 18, 19, 5, 14],   // left-back
        [7, 11, 9, 5, 19],    // back-top
        [7, 19, 18, 6, 15],   // back-bottom-left
        [7, 15, 13, 3, 11]    // back-bottom-right
    ];
    polyhedron(points = pts, faces = faces, convexity = 2);
}

// --- Main Planter ---
module planter() {
    inner_r = body_radius - wall_thickness * 2;
    inner_s = inner_r / body_radius;

    difference() {
        union() {
            // Main dodecahedron body, cut top and bottom
            intersection() {
                geo_body(body_radius);
                translate([0, 0, (top_cut - bottom_cut) / 2])
                    cube([200, 200, top_cut + bottom_cut], center = true);
            }
            // Wide hexagonal rim at top
            translate([0, 0, top_cut - rim_height])
                cylinder(r = rim_width, h = rim_height, $fn = 6);
        }

        // Hollow interior (scaled down dodecahedron)
        translate([0, 0, wall_thickness - bottom_cut])
        scale([inner_s, inner_s, inner_s])
            intersection() {
                geo_body(body_radius);
                translate([0, 0, 60])
                    cube([200, 200, 120], center = true);
            }

        // Hollow the rim interior
        translate([0, 0, top_cut - rim_height - 1])
            cylinder(r = rim_width - wall_thickness * 2, h = rim_height + 2, $fn = 6);

        // Drainage holes
        for (i = [0:drain_hole_count - 1]) {
            a = i * 360 / drain_hole_count + 45;
            translate([drain_hole_spread * cos(a), drain_hole_spread * sin(a), -bottom_cut - 1])
                cylinder(h = wall_thickness + 10, r = drain_hole_r);
        }

        // Chamfer bottom edge
        translate([0, 0, -bottom_cut - 0.1])
        rotate([180, 0, 0])
            cylinder(h = chamfer_height, r1 = 0, r2 = body_radius * 1.2, $fn = 12);
    }
}

// --- Drip Tray ---
module drip_tray() {
    tray_r = body_radius * 0.65 + tray_clearance + tray_wall;
    difference() {
        cylinder(r = tray_r, h = tray_height, $fn = 6);
        translate([0, 0, tray_wall])
            cylinder(r = tray_r - tray_wall, h = tray_height + 1, $fn = 6);
    }
}

// --- Render ---
planter();

// Drip tray placed beside for printing
translate([body_radius * 2.8, 0, bottom_cut])
    drip_tray();
