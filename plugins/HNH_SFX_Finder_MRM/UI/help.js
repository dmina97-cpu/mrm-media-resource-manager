(() => {
  const dialog=document.querySelector('#helpDialog');
  document.querySelector('#helpBtn').onclick=()=>dialog.showModal();
  document.querySelector('#closeHelp').onclick=()=>dialog.close();
  // Keep Enter, Space, I/O and search shortcuts inside this modal.
  dialog.addEventListener('keydown',event=>event.stopPropagation());
  document.querySelector('#contactDeveloper').onclick=async()=>{const status=document.querySelector('#contactStatus');try{await window.hnh.openDeveloperContact();status.textContent='Đã mở Facebook của Phạm Nam trong trình duyệt.';}catch{status.textContent='Chưa mở được trình duyệt. Anh em vào https://www.facebook.com/phamnam22s/ để nhắn Phạm Nam nhé.';}};
})();
