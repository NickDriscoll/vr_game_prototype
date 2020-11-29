import re, os

r_expressions = ["albedo|Base_Color", "normal", "roughness"]
out_names = ["albedo", "normal", "roughness"]

for entry in os.scandir("."):
	if entry.is_dir():
		for e in os.scandir(entry.path):
			for i in range(0, len(r_expressions)):
				if re.search(r_expressions[i], e.name, re.IGNORECASE):
					new_path = "%s/%s.png" % (entry.path, out_names[i])
					os.rename(e.path, new_path)
					print("Moved %s to %s" % (e.path, new_path))
					break