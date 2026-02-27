// Dodecahedron with boolean difference — regression test for
// "Conflicting edge" crash with N-gon polyhedron faces.

phi = (1 + sqrt(5)) / 2;

points = [
    [ 1,  1,  1], [ 1,  1, -1], [ 1, -1,  1], [ 1, -1, -1],
    [-1,  1,  1], [-1,  1, -1], [-1, -1,  1], [-1, -1, -1],
    [0,  1/phi,  phi], [0,  1/phi, -phi], [0, -1/phi,  phi], [0, -1/phi, -phi],
    [ 1/phi,  phi, 0], [ 1/phi, -phi, 0], [-1/phi,  phi, 0], [-1/phi, -phi, 0],
    [ phi, 0,  1/phi], [ phi, 0, -1/phi], [-phi, 0,  1/phi], [-phi, 0, -1/phi]
];

faces = [
    [0,8,10,2,16],  [0,16,17,1,12], [0,12,14,4,8],
    [1,17,3,11,9],  [1,9,5,14,12],  [2,10,6,15,13],
    [2,13,3,17,16], [3,13,15,7,11], [4,14,5,19,18],
    [4,18,6,10,8],  [5,9,11,7,19],  [6,18,19,7,15]
];

difference() {
    polyhedron(points=points, faces=faces);
    sphere(r=0.5, $fn=16);
}
