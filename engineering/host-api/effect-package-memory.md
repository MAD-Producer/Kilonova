# Effect package CPU memory observation

Issue: #60, partial ownership coverage. The explicit Session `resource_memory`
query includes `cpu.effectPackages` and its known payload/capacity in the CPU
subtotal. Ordinary snapshots and frame preparation do not run this enumeration.

One borrowed query observes the Session plan builder, live preview renderer and
editor renderer together. Registry and plan-program references to the same
actual Arc package count once, across all these owners. Equal hashes do not
collapse independent allocations. Snapshot IDs never serialize pointers, retain
Arcs, load files, compile shaders, initialize graphics or submit GPU work.

Three categories count separately: the compressed archive Vec, expanded file
Vecs, and package-owned WGSL/GLSL String buffers. Payload uses lengths; capacity
uses actual backing capacity. These categories coexist in RAM, so expanded
buffers are not inferred from compressed file size. Reference counts distinguish
registries and plan programs. Dropping one owner preserves capacity while another
observed owner still retains the package. External/global package ownership is
outside this Session subtotal after all observed owners release it.

The observation retains at most 1024 unique package entries and at most 160
characters per plugin/hash context. Further unobserved references are counted,
`allPackageOwnersObserved=false` and unknown bytes remain null. A later query
starts fresh. Counters and the temporary identity map are not lifetime registries.

Known CPU totals now include startup's built-in packages, even before a GPU
exists. Scratch/cache tests therefore assert their exact additional capacity
above the retained package capacity and keep testing payload clearing, capacity
release and public bridge equality. This is a new observed category, not a change
to caching or resource admission.

`cpu.effectProgramShaders` separately measures program-owned compiled WGSL/GLSL
String buffers, including independent copies of package shader text. One query
deduplicates actual shader Arcs across retained builders; equal text in distinct
allocations still counts separately. At most 4096 unique shaders and eight
160-character program contexts per shader are retained. Additional unobserved
references and omitted contexts are counted. Shader source text is never returned.

Manifest/map keys, container/allocator/Arc overhead, external package/shader
owners, GPU modules and driver storage remain
unknown/excluded. The CPU observation is not process RSS or GPU VRAM and keeps
`completeCoverage=false`. Texture capacity remains a separate domain.
