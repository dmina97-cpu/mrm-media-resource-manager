// Popup khi bấm gỡ thư mục: chỉ ra cách giữ tag / yêu thích (đổi đường dẫn, quét lại, Loại quét, Nối lại) trước khi gỡ.
(() => {
  const dialog = document.querySelector('#libraryRemoveDialog');
  const body = document.querySelector('#libraryRemoveBody');
  const actions = document.querySelector('#libraryRemoveActions');
  let busy = false;
  const esc = s => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
  const opt = (title, text) => `<li><b>${title}</b><span>${text}</span></li>`;
  const button = (label, cls, fn) => { const b = document.createElement('button'); b.textContent = label; if (cls) b.className = cls; b.onclick = fn; return b; };

  dialog.addEventListener('keydown', e => e.stopPropagation());
  dialog.addEventListener('cancel', e => { if (busy) e.preventDefault(); });

  window.HnhLibraryRemove = {
    /** lib: thư mục; mrm: đang dùng chung dữ liệu với MRM; remove: hàm gỡ (chế độ độc lập) */
    open(lib, { mrm, remove }) {
      if (busy) return;
      document.querySelector('#libraryRemovePath').textContent = lib.path;
      actions.textContent = '';
      const types = () => { dialog.close(); window.HnhLibraryTypes.open(lib); };
      if (mrm) {
        document.querySelector('#libraryRemoveTitle').textContent = 'Gỡ thư mục này?';
        body.innerHTML = '<p class="lr-lead">Thư mục đang <b>dùng chung dữ liệu với MRM</b>. Gỡ sẽ xóa tag, yêu thích, ghi chú của cả thư mục — nên chỉ gỡ được trong MRM (MRM báo trước dữ liệu sẽ mất và tự sao lưu).</p>'
          + '<p class="lr-q">Bạn chỉ đang sắp xếp lại? Không cần gỡ:</p><ul class="lr-opts">'
          + opt('Đã di chuyển, đổi tên thư mục hoặc sang ổ khác', 'MRM → Libraries &amp; Settings → <b>Đổi đường dẫn</b>: giữ nguyên tag, yêu thích.')
          + opt('Sắp xếp lại thư mục con bên trong', 'Chỉ cần <b>Quét lại</b>: file di chuyển tự được nối lại, giữ tag.')
          + opt('Gom sang thư mục khác', 'Thêm và quét thư mục mới <b>trước</b>, rồi mới gỡ thư mục cũ trong MRM: file đã chuyển mang theo tag, yêu thích.')
          + opt('Chỉ muốn bỏ âm thanh (hoặc hình, video) của thư mục này', 'Dùng <b>Loại quét</b>: ẩn loại đó, tag và yêu thích vẫn giữ.')
          + '</ul><p class="hint">Vẫn muốn gỡ: mở MRM → Libraries &amp; Settings → <b>Gỡ</b> ở thư mục này. Plugin tự cập nhật theo.</p>';
        actions.append(button('Loại quét…', 'secondary', types), button('Đóng', '', () => dialog.close()));
      } else {
        document.querySelector('#libraryRemoveTitle').textContent = 'Gỡ thư mục khỏi plugin?';
        body.innerHTML = '<p class="lr-lead">File thật trên ổ đĩa <b>không</b> bị ảnh hưởng. Tag và yêu thích của file trong thư mục được giữ lại — thêm lại đúng thư mục này là hiện lại như cũ.</p>'
          + '<p class="lr-q">Không cần gỡ nếu:</p><ul class="lr-opts">'
          + opt('Đã di chuyển, đổi tên thư mục hoặc sang ổ khác', 'Dùng <b>Nối lại</b>: chuyển tag, yêu thích, ngày thêm sang vị trí mới.')
          + opt('Chỉ muốn bỏ âm thanh (hoặc hình, video) của thư mục này', 'Dùng <b>Loại quét</b>.')
          + '</ul>';
        const go = button('Gỡ thư mục', 'danger', async () => {
          busy = true;
          for (const b of actions.querySelectorAll('button')) b.disabled = true;
          try { await remove(); dialog.close(); } catch (e) { body.insertAdjacentHTML('beforeend', `<p class="lr-err">${esc(String(e?.message || e).replace(/^Error invoking remote method '[^']*': (?:Error: )?/, ''))}</p>`); }
          finally { busy = false; for (const b of actions.querySelectorAll('button')) b.disabled = false; }
        });
        actions.append(button('Nối lại…', 'secondary', () => { dialog.close(); window.HnhRelink.open(lib.path); }), button('Loại quét…', 'secondary', types), button('Hủy', 'secondary', () => dialog.close()), go);
      }
      dialog.showModal();
    }
  };
})();
