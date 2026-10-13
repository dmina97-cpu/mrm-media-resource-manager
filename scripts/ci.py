"""Theo dõi GitHub Actions và phát hành MRM từ máy dev (không cần gh CLI).

  python scripts/ci.py status                      lần chạy mới nhất + từng bước
  python scripts/ci.py logs [run_id]               log quanh chỗ lỗi (mặc định: lần chạy mới nhất)
  python scripts/ci.py dry                         chạy thử workflow Windows, không phát hành
  python scripts/ci.py mac vX.Y.Z                  build bản macOS và gắn vào release vX.Y.Z
  python scripts/ci.py release vX.Y.Z              liệt kê file của release
  python scripts/ci.py publish vX.Y.Z RUN [RUN..]  tạo release từ artifact của các lần chạy
                                                   (dùng khi bước tạo release trên CI lỗi quyền)

Đăng nhập: lấy token từ `git credential` của máy (tài khoản đang push được repo). Không in token ra màn hình.
"""
import hashlib, io, json, os, subprocess, sys, time, urllib.error, urllib.request, zipfile

sys.stdout.reconfigure(encoding='utf-8')
REPO = 'dmina97-cpu/mrm-media-resource-manager'
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def token():
    out = subprocess.run(['git', 'credential', 'fill'], input='protocol=https\nhost=github.com\n\n', capture_output=True, text=True, cwd=ROOT).stdout
    for line in out.splitlines():
        if line.startswith('password='):
            return line[len('password='):]
    sys.exit('Không lấy được thông tin đăng nhập GitHub từ git credential.')


TOKEN = token()


def api(method, path, data=None, raw=False, base='https://api.github.com/repos/' + REPO):
    h = {'Accept': 'application/vnd.github+json', 'User-Agent': 'MRM-ci', 'X-GitHub-Api-Version': '2022-11-28'}
    body = None
    if isinstance(data, (bytes, bytearray)):
        body, h['Content-Type'] = data, 'application/octet-stream'
    elif data is not None:
        body, h['Content-Type'] = json.dumps(data).encode(), 'application/json'
    req = urllib.request.Request(base + path, data=body, headers=h, method=method)
    # log và artifact chuyển hướng sang kho lưu trữ khác: không gửi token theo
    req.add_unredirected_header('Authorization', 'Bearer ' + TOKEN)
    try:
        with urllib.request.urlopen(req, timeout=900) as r:
            b = r.read()
            return r.status, (b if raw else (json.loads(b) if b else None))
    except urllib.error.HTTPError as e:
        b = e.read()
        return e.code, (b.decode(errors='replace') if b else None)


def latest_run():
    return api('GET', '/actions/runs?per_page=1')[1]['workflow_runs'][0]


def show(run):
    print('run', run['id'], run['name'], run['event'], run['head_branch'], run['status'], run['conclusion'], run['html_url'])
    for j in api('GET', f"/actions/runs/{run['id']}/jobs")[1]['jobs']:
        print('job', j['name'], j['status'], j['conclusion'])
        for s in j['steps']:
            print(f"   {s['number']:2} {s['name'][:44]:44} {s['status']:11} {s['conclusion']}")


def logs(run_id):
    for j in api('GET', f'/actions/runs/{run_id}/jobs')[1]['jobs']:
        st, b = api('GET', f"/actions/jobs/{j['id']}/logs", raw=True)
        lines = (b.decode('utf-8', 'replace') if isinstance(b, bytes) else str(b)).splitlines()
        hits = [i for i, l in enumerate(lines) if '##[error]' in l or 'error[' in l or 'FAILED' in l or 'panicked' in l]
        for i in hits[:5]:
            print('\n'.join(l[29:] for l in lines[max(0, i - 25): i + 5]))
            print('-' * 60)
        if not hits:
            print('\n'.join(l[29:] for l in lines[-40:]))


def release(tag):
    st, r = api('GET', f'/releases/tags/{tag}')
    if st != 200:
        print('chưa có release', tag, st)
        return None
    print(r['name'], '| draft:', r['draft'], '|', r['html_url'])
    for a in r['assets']:
        print(f"  {a['name']:44} {a['size'] / 1048576:6.1f} MB  {a['state']}  {(a.get('digest') or '')[:23]}")
    return r


def publish(tag, run_ids):
    """Tải artifact của các lần chạy -> tạo release nháp -> tải file lên -> công bố (người dùng không thấy release thiếu file)."""
    files = {}
    for rid in run_ids:
        for art in api('GET', f'/actions/runs/{rid}/artifacts')[1]['artifacts']:
            st, b = api('GET', f"/actions/artifacts/{art['id']}/zip", raw=True)
            assert st == 200, (art['name'], st)
            with zipfile.ZipFile(io.BytesIO(b)) as z:
                for n in z.namelist():
                    if not n.endswith('/'):
                        files[os.path.basename(n)] = z.read(n)
    assert files, 'không có artifact nào'
    notes_path = os.path.join(ROOT, 'release-notes', tag + '.md')
    conf = json.load(open(os.path.join(ROOT, 'src-tauri', 'tauri.conf.json'), encoding='utf-8'))
    plugin = json.load(open(os.path.join(ROOT, 'plugins', 'HNH_SFX_Finder_MRM', 'package.json'), encoding='utf-8'))
    st, r = api('GET', f'/releases/tags/{tag}')
    if st != 200:
        st, r = api('POST', '/releases', {'tag_name': tag, 'draft': True, 'name': f"MRM {conf['version']} + Plugin HNH SFX Finder {plugin['version']}",
                                         'body': open(notes_path, encoding='utf-8').read() if os.path.exists(notes_path) else ''})
        assert st == 201, (st, r)
    have = {a['name'] for a in r.get('assets', [])}
    for name, b in files.items():
        if name in have:
            print('đã có', name)
            continue
        st, _ = api('POST', f"/releases/{r['id']}/assets?name={name}", b, base='https://uploads.github.com/repos/' + REPO)
        print('upload', name, st, 'sha256:' + hashlib.sha256(b).hexdigest()[:16])
    st, r = api('PATCH', f"/releases/{r['id']}", {'draft': False, 'make_latest': 'true'})
    print('công bố', st, r.get('html_url'))


cmd, args = (sys.argv[1] if len(sys.argv) > 1 else 'status'), sys.argv[2:]
if cmd == 'status':
    show(latest_run())
elif cmd == 'logs':
    logs(args[0] if args else latest_run()['id'])
elif cmd == 'dry':
    print('dispatch', api('POST', '/actions/workflows/release.yml/dispatches', {'ref': 'main', 'inputs': {'publish': 'false'}})[0])
    time.sleep(6)
    show(latest_run())
elif cmd == 'mac':
    assert args, 'thiếu tag, vd: mac v0.12.1'
    print('dispatch', api('POST', '/actions/workflows/release-macos.yml/dispatches', {'ref': 'main', 'inputs': {'tag': args[0], 'publish': 'true'}})[0])
    time.sleep(6)
    show(latest_run())
elif cmd == 'release':
    release(args[0])
elif cmd == 'publish':
    publish(args[0], args[1:])
else:
    print(__doc__)
