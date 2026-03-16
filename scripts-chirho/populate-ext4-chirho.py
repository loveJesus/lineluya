# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16
#
# Populate an ext4 image with files from an Alpine rootfs tarball using debugfs.
# This avoids Docker volume mount and pipe corruption issues on macOS.
#
# Usage: python3 scripts-chirho/populate-ext4-chirho.py \
#          target/alpine-virtio-chirho/alpine-virtio-chirho.img \
#          target/alpine-virtio-chirho/alpine-minirootfs-3.21.0-x86_64.tar.gz

import os
import sys
import tarfile
import subprocess
import tempfile

DEBUGFS_CHIRHO = "/opt/homebrew/opt/e2fsprogs/sbin/debugfs"

def run_debugfs_chirho(img_path_chirho, commands_chirho):
    """Run a batch of debugfs commands on the image."""
    cmd_text_chirho = "\n".join(commands_chirho) + "\n"
    result_chirho = subprocess.run(
        [DEBUGFS_CHIRHO, "-w", img_path_chirho],
        input=cmd_text_chirho,
        capture_output=True,
        text=True,
    )
    if result_chirho.returncode != 0:
        print(f"debugfs error: {result_chirho.stderr[:500]}", file=sys.stderr)

def populate_image_chirho(img_path_chirho, tarball_path_chirho):
    """Extract tarball contents into ext4 image using debugfs."""
    print(f"[EXT4] Populating {img_path_chirho} from {tarball_path_chirho}...")

    # Track directories we've already created
    created_dirs_chirho = set(["/", ""])

    with tarfile.open(tarball_path_chirho, "r:gz") as tar_chirho:
        members_chirho = tar_chirho.getmembers()
        print(f"[EXT4] {len(members_chirho)} entries in tarball")

        # First pass: create all directories
        dir_cmds_chirho = []
        for member_chirho in members_chirho:
            name_chirho = member_chirho.name.lstrip("./")
            if not name_chirho:
                continue
            path_chirho = "/" + name_chirho

            if member_chirho.isdir():
                if path_chirho not in created_dirs_chirho:
                    dir_cmds_chirho.append(f"mkdir {path_chirho}")
                    created_dirs_chirho.add(path_chirho)
            else:
                # Ensure parent directories exist
                parent_chirho = os.path.dirname(path_chirho)
                parts_chirho = parent_chirho.split("/")
                for i_chirho in range(2, len(parts_chirho) + 1):
                    partial_chirho = "/".join(parts_chirho[:i_chirho])
                    if partial_chirho and partial_chirho not in created_dirs_chirho:
                        dir_cmds_chirho.append(f"mkdir {partial_chirho}")
                        created_dirs_chirho.add(partial_chirho)

        if dir_cmds_chirho:
            print(f"[EXT4] Creating {len(dir_cmds_chirho)} directories...")
            # Process in batches of 200
            for i_chirho in range(0, len(dir_cmds_chirho), 200):
                batch_chirho = dir_cmds_chirho[i_chirho:i_chirho + 200]
                run_debugfs_chirho(img_path_chirho, batch_chirho)

        # Second pass: write regular files
        file_count_chirho = 0
        symlink_count_chirho = 0
        skip_count_chirho = 0

        with tempfile.TemporaryDirectory() as tmpdir_chirho:
            file_cmds_chirho = []

            for member_chirho in members_chirho:
                name_chirho = member_chirho.name.lstrip("./")
                if not name_chirho:
                    continue
                path_chirho = "/" + name_chirho

                if member_chirho.isfile():
                    # Extract file to temp dir, then write into image
                    try:
                        tar_chirho.extract(member_chirho, tmpdir_chirho)
                        host_path_chirho = os.path.join(tmpdir_chirho, member_chirho.name)
                        if os.path.exists(host_path_chirho):
                            file_cmds_chirho.append(f"write {host_path_chirho} {path_chirho}")
                            file_count_chirho += 1
                    except Exception as e_chirho:
                        skip_count_chirho += 1

                elif member_chirho.issym():
                    file_cmds_chirho.append(f"symlink {path_chirho} {member_chirho.linkname}")
                    symlink_count_chirho += 1

                elif member_chirho.islnk():
                    # Hard link — debugfs doesn't support ln directly,
                    # use link command
                    target_chirho = "/" + member_chirho.linkname.lstrip("./")
                    file_cmds_chirho.append(f"link {target_chirho} {path_chirho}")
                    file_count_chirho += 1

                # Process files in batches
                if len(file_cmds_chirho) >= 100:
                    run_debugfs_chirho(img_path_chirho, file_cmds_chirho)
                    file_cmds_chirho = []

            # Flush remaining
            if file_cmds_chirho:
                run_debugfs_chirho(img_path_chirho, file_cmds_chirho)

        print(f"[EXT4] Written: {file_count_chirho} files, {symlink_count_chirho} symlinks, {skip_count_chirho} skipped")

    # Write configuration files
    write_config_files_chirho(img_path_chirho)

    # Verify
    result_chirho = subprocess.run(
        [DEBUGFS_CHIRHO, "-R", "ls /bin", img_path_chirho],
        capture_output=True, text=True,
    )
    bin_entries_chirho = len(result_chirho.stdout.strip().split("\n"))
    print(f"[EXT4] /bin has {bin_entries_chirho} entries")

    result_chirho = subprocess.run(
        [DEBUGFS_CHIRHO, "-R", "ls /lib", img_path_chirho],
        capture_output=True, text=True,
    )
    print(f"[EXT4] /lib contents: {result_chirho.stdout.strip()[:200]}")


def write_config_files_chirho(img_path_chirho):
    """Write hostname, resolv.conf, etc. into the image."""
    with tempfile.TemporaryDirectory() as tmpdir_chirho:
        cmds_chirho = []

        # hostname
        hostname_path_chirho = os.path.join(tmpdir_chirho, "hostname")
        with open(hostname_path_chirho, "w") as f_chirho:
            f_chirho.write("lineluya-chirho\n")
        cmds_chirho.append(f"write {hostname_path_chirho} /etc/hostname")

        # resolv.conf
        resolv_path_chirho = os.path.join(tmpdir_chirho, "resolv.conf")
        with open(resolv_path_chirho, "w") as f_chirho:
            f_chirho.write("nameserver 8.8.8.8\nnameserver 1.1.1.1\n")
        cmds_chirho.append(f"write {resolv_path_chirho} /etc/resolv.conf")

        # test python script
        py_path_chirho = os.path.join(tmpdir_chirho, "hello_chirho.py")
        with open(py_path_chirho, "w") as f_chirho:
            f_chirho.write('# John 3:16\nprint("Hallelujah! Python3 on Lineluya!")\n')
        cmds_chirho.append("mkdir /root")
        cmds_chirho.append(f"write {py_path_chirho} /root/hello_chirho.py")

        # test SQL script
        sql_path_chirho = os.path.join(tmpdir_chirho, "test_chirho.sql")
        with open(sql_path_chirho, "w") as f_chirho:
            f_chirho.write("CREATE TABLE t(id INTEGER PRIMARY KEY, msg TEXT);\n")
            f_chirho.write("INSERT INTO t VALUES(1,'Hallelujah!');\n")
            f_chirho.write("SELECT * FROM t;\n")
        cmds_chirho.append(f"write {sql_path_chirho} /root/test_chirho.sql")

        run_debugfs_chirho(img_path_chirho, cmds_chirho)
        print("[EXT4] Config files written")


def install_packages_chirho(img_path_chirho):
    """Download and install Alpine packages into the ext4 image using debugfs.
    Downloads .apk files, extracts them, and writes into the image."""
    import urllib.request
    import io

    MIRROR_CHIRHO = "https://dl-cdn.alpinelinux.org/alpine/v3.23/main/x86_64"

    # Packages to install (and their deps that aren't already in minirootfs)
    packages_chirho = [
        "sqlite-libs-3.51.2-r0",
        "sqlite-3.51.2-r0",
        "ncurses-terminfo-base-6.5_p20251123-r0",
        "libncursesw-6.5_p20251123-r0",
        "readline-8.3.1-r0",
    ]

    with tempfile.TemporaryDirectory() as tmpdir_chirho:
        for pkg_chirho in packages_chirho:
            url_chirho = f"{MIRROR_CHIRHO}/{pkg_chirho}.apk"
            apk_path_chirho = os.path.join(tmpdir_chirho, f"{pkg_chirho}.apk")

            print(f"[APK] Downloading {pkg_chirho}...")
            try:
                urllib.request.urlretrieve(url_chirho, apk_path_chirho)
            except Exception as e_chirho:
                print(f"[APK] Failed to download {pkg_chirho}: {e_chirho}", file=sys.stderr)
                continue

            # .apk files are gzipped tarballs
            extract_dir_chirho = os.path.join(tmpdir_chirho, pkg_chirho)
            os.makedirs(extract_dir_chirho, exist_ok=True)

            try:
                with tarfile.open(apk_path_chirho, "r:gz") as apk_tar_chirho:
                    apk_tar_chirho.extractall(extract_dir_chirho)
            except Exception as e_chirho:
                print(f"[APK] Failed to extract {pkg_chirho}: {e_chirho}", file=sys.stderr)
                continue

            # Write extracted files into the ext4 image
            file_cmds_chirho = []
            for root_chirho, dirs_chirho, files_chirho in os.walk(extract_dir_chirho):
                rel_root_chirho = os.path.relpath(root_chirho, extract_dir_chirho)
                if rel_root_chirho == ".":
                    img_root_chirho = ""
                else:
                    img_root_chirho = "/" + rel_root_chirho

                # Skip .PKGINFO, .SIGN, .post-install etc
                if img_root_chirho.startswith("/."):
                    continue

                # Create directories
                for d_chirho in dirs_chirho:
                    if d_chirho.startswith("."):
                        continue
                    dir_path_chirho = f"{img_root_chirho}/{d_chirho}"
                    file_cmds_chirho.append(f"mkdir {dir_path_chirho}")

                # Write files
                for fname_chirho in files_chirho:
                    if fname_chirho.startswith("."):
                        continue
                    host_file_chirho = os.path.join(root_chirho, fname_chirho)
                    img_file_chirho = f"{img_root_chirho}/{fname_chirho}"
                    if os.path.isfile(host_file_chirho) and not os.path.islink(host_file_chirho):
                        file_cmds_chirho.append(f"write {host_file_chirho} {img_file_chirho}")

            if file_cmds_chirho:
                run_debugfs_chirho(img_path_chirho, file_cmds_chirho)
                print(f"[APK] Installed {pkg_chirho} ({len(file_cmds_chirho)} operations)")

    # Verify sqlite3
    result_chirho = subprocess.run(
        [DEBUGFS_CHIRHO, "-R", "stat /usr/bin/sqlite3", img_path_chirho],
        capture_output=True, text=True,
    )
    if "Type: regular" in result_chirho.stdout:
        print("[APK] sqlite3 verified on image!")
    else:
        print("[APK] WARNING: sqlite3 not found on image", file=sys.stderr)


if __name__ == "__main__":
    if len(sys.argv) < 3:
        print(f"Usage: {sys.argv[0]} <image> <tarball> [--install-packages]")
        sys.exit(1)

    img_chirho = sys.argv[1]
    tarball_chirho = sys.argv[2]
    install_pkgs_chirho = "--install-packages" in sys.argv

    if not os.path.exists(DEBUGFS_CHIRHO):
        print(f"ERROR: debugfs not found at {DEBUGFS_CHIRHO}")
        print("Install with: brew install e2fsprogs")
        sys.exit(1)

    populate_image_chirho(img_chirho, tarball_chirho)

    if install_pkgs_chirho:
        install_packages_chirho(img_chirho)

    print("[EXT4] Done! Praise Jesus!")
