# Makes the USDZ files of the Mac app's models from their sources, with Blender: a model posed
# where it looks best, simplified so that the app stays small, without the cameras, lights and
# backdrop its file may have.
#
#     blender -b -P macos/app/models/blender_usdz.py -- SOURCE OUT.usdz SECONDS [MAXFACES]
#
# The two Nothing and CMF models were made this way (their sources are not in the repository):
#
#     … -- "Anim Nothing earbuds(pack).glb" headphones-cmf-buds-pro-2.usdz 7.3 8000
#                                     # the lid open and both buds out, 7.3 s into its animation
#     … -- model.blend headphones-headphone-1.usdz 0 2500
#
# Then `swift macos/app/models/portrait.swift cmf-buds-pro-2` (and headphone-1) draws the panel's
# still of each.
import sys
import bpy

args = sys.argv[sys.argv.index("--") + 1:]
source, target, seconds = args[:3]
most = int(args[3]) if len(args) > 3 else 0
if source.endswith(".blend"):
    bpy.ops.wm.open_mainfile(filepath=source)
else:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=source)
scene = bpy.context.scene
scene.frame_set(round(float(seconds) * scene.render.fps / scene.render.fps_base))
for thing in list(scene.objects):
    if thing.type in {"CAMERA", "LIGHT"} or (thing.type == "MESH" and max(thing.dimensions) > 1.0):
        print("left out", thing.name, thing.type)
        bpy.data.objects.remove(thing, do_unlink=True)
depsgraph = bpy.context.evaluated_depsgraph_get()
total = 0
for part in [o for o in scene.objects if o.type == "MESH"]:
    faces = len(part.evaluated_get(depsgraph).data.polygons)
    if most and faces > most:
        simpler = part.modifiers.new("simpler", "DECIMATE")
        simpler.ratio = most / faces
        faces = most
    total += faces
print("about", total, "faces")
wanted = {
    "export_animation": False, "export_materials": True, "generate_preview_surface": True,
    "overwrite_textures": True, "selected_objects_only": False, "convert_orientation": True,
    "export_global_forward_selection": "NEGATIVE_Z", "export_global_up_selection": "Y",
    "evaluation_mode": "RENDER", "export_lights": False, "export_cameras": False,
}
known = bpy.ops.wm.usd_export.get_rna_type().properties.keys()
bpy.ops.wm.usd_export(filepath=target, **{k: v for k, v in wanted.items() if k in known})
