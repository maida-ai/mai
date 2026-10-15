"""Tiny fixture-only PEP 517 backend; no downloaded build dependencies."""
import base64
import csv
import hashlib
import io
import pathlib
import zipfile


def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    name = pathlib.Path.cwd().name
    version = "1.0.0"
    info = f"{name}-{version}.dist-info"
    files = {
        f"{name}.py": pathlib.Path(f"{name}.py").read_bytes(),
        f"{info}/METADATA": f"Metadata-Version: 2.1\nName: {name}\nVersion: {version}\n".encode(),
        f"{info}/WHEEL": b"Wheel-Version: 1.0\nGenerator: s6-fixture\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    }
    rows = []
    for path, data in files.items():
        digest = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b"=").decode()
        rows.append((path, f"sha256={digest}", str(len(data))))
    rows.append((f"{info}/RECORD", "", ""))
    record = io.StringIO()
    csv.writer(record, lineterminator="\n").writerows(rows)
    files[f"{info}/RECORD"] = record.getvalue().encode()
    filename = f"{name}-{version}-py3-none-any.whl"
    with zipfile.ZipFile(pathlib.Path(wheel_directory) / filename, "w") as archive:
        for path, data in sorted(files.items()):
            archive.writestr(zipfile.ZipInfo(path, (2020, 1, 1, 0, 0, 0)), data)
    return filename
