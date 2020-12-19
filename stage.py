import subprocess

cargo_proc = subprocess.run(["cargo", "build", "--release"], check=True)
print(cargo_proc.returncode)