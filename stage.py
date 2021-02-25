from py7zr import SevenZipFile
import subprocess, os, shutil, time

start_time = time.time()

staging_dir = "dist"
dirs = ["materials", "models", "shaders", "skyboxes"]

if os.path.exists(staging_dir):
	shutil.rmtree(staging_dir)

cargo_proc = subprocess.run(["cargo", "build", "--release"], check=True)

os.mkdir(staging_dir)

for d in dirs:
	shutil.copytree(d, "%s/%s" % (staging_dir, d))

shutil.copy("target/release/hot_chickens.exe", "%s/" % staging_dir)

#Compress the build into a 7z archive
print("Compressing build...")
with SevenZipFile("hot_chickens.7z", "w") as archive:
	archive.writeall(staging_dir)

#Cleanup
shutil.rmtree(staging_dir)

print("Done! in %.4f seconds" % (time.time() - start_time))