"""Package only the native executable, public handbooks and setup documentation."""
from pathlib import Path
import zipfile,hashlib,shutil
root=Path(__file__).resolve().parent.parent
exe=root/"target/release/eplab-rust.exe"
if not exe.exists():raise SystemExit("Build the Windows release first.")
out=root/"work/dist";out.mkdir(parents=True,exist_ok=True)
archive=out/"EPlab-Rust-v0.1.0-windows-x64.zip"
instructions="""EP Lab Rust 0.1 - Windows x64

1. Extract the ZIP.
2. Create a NEW PostgreSQL database (do not use the TypeScript database).
3. Copy .env.example to .env and set DATABASE_URL.
4. Open PowerShell in this folder and run .\\eplab-rust.exe serve
5. Open http://127.0.0.1:3200

To create an admin, set ADMIN_EMAIL, ADMIN_NAME and ADMIN_PASSWORD privately in your environment,
then run .\\eplab-rust.exe admin. Remove ADMIN_PASSWORD after use.
No account password or database password is included in this archive.

Local development mail is written to data/mail; it is never served by HTTP.
For public hosting configure HTTPS origin and SMTP. Read docs/SECURITY.md.
The executable needs Windows system DLLs and PostgreSQL; it does not need Node.js or Rust installed.
"""
with zipfile.ZipFile(archive,"w",zipfile.ZIP_DEFLATED,compresslevel=9) as z:
 z.write(exe,"eplab-rust.exe")
 for name in [".env.example","README.md","NOTICE.md"]:
  z.write(root/name,name)
 for p in list((root/"content/handbooks").rglob("*"))+list((root/"docs").rglob("*")):
  if p.is_file():z.write(p,p.relative_to(root).as_posix())
 z.writestr("START-HERE.txt",instructions)
checksum=hashlib.sha256(archive.read_bytes()).hexdigest()
(out/"SHA256SUMS.txt").write_text(checksum+"  "+archive.name+"\n",encoding="utf-8")
print(archive.name,archive.stat().st_size,"bytes; SHA256",checksum)
