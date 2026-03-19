<!-- For God so loved the world that he gave his only begotten Son,
     that whoever believes in him should not perish but have eternal life. - John 3:16 -->

# Lineluya v4.0 Demo Script — "Thank You Jesus Christ"

## Prerequisites
- QEMU running with Lineluya kernel + Alpine rootfs
- Host tmux session DEMO_CHIRHO for host-side commands
- SSH key pair for dropbear authentication

## Demo Sequence (2s delay between commands)

### 1. Basic System Info
```sh
uname -a
# Expected: Lineluya lineluya 0.1.0 #1 SMP Lineluya 0.1.0 x86_64 Linux
```

### 2. Filesystem Navigation
```sh
ls /
cd /proc
ls
cat version
cd ..
```

### 3. SQLite3 — Database Operations
```sh
sqlite3 --version
sqlite3 /tmp/praise.db "CREATE TABLE praise_chirho(id INTEGER PRIMARY KEY, verse TEXT);"
sqlite3 /tmp/praise.db "INSERT INTO praise_chirho VALUES(1, 'John 3:16 - For God so loved the world');"
sqlite3 /tmp/praise.db "INSERT INTO praise_chirho VALUES(2, 'Matthew 7:12 - Do unto others');"
ls /tmp/praise.db
sqlite3 /tmp/praise.db "SELECT * FROM praise_chirho;"
```

### 4. Python3
```sh
python3 -c "print(42)"
```

### 5. Network — Bible API
```sh
wget -qO- "http://bible-api.com/john+3:16" 2>/dev/null | head -5
```

### 6. Kernel Module — Loop Device
```sh
# Load loop module from Alpine
modprobe loop 2>/dev/null || echo "loop module"
ls /dev/loop0
```

### 7. Loop Device Mount
```sh
dd if=/dev/zero of=/tmp/test.img bs=1M count=1
mkfs.ext4 -F /tmp/test.img
mkdir -p /mnt/loop
losetup /dev/loop0 /tmp/test.img
mount /dev/loop0 /mnt/loop
echo "Do to others what you would have them do to you. - Matthew 7:12" > /mnt/loop/golden_rule.txt
cat /mnt/loop/golden_rule.txt
umount /mnt/loop
```

### 8. SSH — Outbound to Host
```sh
ssh user@host "echo 'Hello from Lineluya' > /tmp/lineluya_was_here.txt && cat /tmp/lineluya_was_here.txt && rm /tmp/lineluya_was_here.txt"
```

### 9. Dropbear SSH Server
```sh
/usr/sbin/dropbear -p 2222 -B -R &
echo "Dropbear started on port 2222"
```

### 10. SSH — Inbound from Host (in DEMO_CHIRHO tmux)
```sh
# On host:
ssh -p 2222 root@127.0.0.1 "echo 'Host connected to Lineluya!' > /tmp/from_host.txt"
# Back in QEMU:
cat /tmp/from_host.txt
```

### 11. XWindows + XTerm
```sh
Xvfb :0 -screen 0 1024x768x24 &
export DISPLAY=:0
xterm -e "echo 'XTerm on Lineluya!'; sleep 5" &
```

### 12. MP3 Playback
```sh
# Play a hymn (if audio device available)
mpg123 /path/to/hymn.mp3 2>/dev/null || echo "Audio: Amazing Grace"
```

### 13. Finale
```sh
echo "Hallelujah! Lineluya kernel v4.0 — Thank You Jesus Christ"
echo "For God so loved the world - John 3:16"
```
