"""Module-definition file for vulkan-1.dll (the Vulkan loader every GPU driver installs on Windows), made from the
Vulkan headers: every vk* entry point declared there. scripts/build_windows.sh turns it into the import library
vulkan-1.lib with llvm-dlltool, so ggml-vulkan.dll can be linked on a Mac. Names the loader does not export are
harmless unless used (ggml loads nearly everything through vkGetInstanceProcAddr).

    python3 scripts/dev/vulkan_def.py vulkan_core.h vulkan_win32.h > vulkan-1.def
"""
import re
import sys

names = sorted({m for f in sys.argv[1:] for m in re.findall(r"VKAPI_ATTR \w+\*? VKAPI_CALL (vk\w+)\(", open(f).read())})
if not names:
    sys.exit("no vk* prototypes found")
print("LIBRARY vulkan-1.dll\nEXPORTS\n" + "\n".join(names))
