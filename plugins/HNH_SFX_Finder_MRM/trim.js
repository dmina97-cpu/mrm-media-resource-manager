// Seconds are converted once at the Resolve boundary. Out is exclusive in the UI.
function validateRange(range) {
  const {inSeconds, outSeconds, duration} = range;
  if (![inSeconds,outSeconds,duration].every(Number.isFinite) || duration<=0 || inSeconds<0 || outSeconds<=inSeconds || outSeconds>duration+0.001) throw new Error('Khoảng In/Out không hợp lệ.');
}
function trimFrames(range, fps, frameCount) {
  if (!range) return null;
  validateRange(range);
  if(!Number.isFinite(fps)||fps<=0)throw new Error('Không đọc được frame rate nguồn.');
  const {inSeconds,outSeconds}=range;
  const startFrame=Math.round(inSeconds*fps);
  let endExclusive=Math.round(outSeconds*fps);
  if(Number.isFinite(frameCount)&&frameCount>0)endExclusive=Math.min(endExclusive,Math.floor(frameCount));
  if(endExclusive<=startFrame)throw new Error('Đoạn chọn phải dài ít nhất một frame của nguồn âm thanh.');
  return {startFrame,endFrame:endExclusive-1};
}
module.exports={trimFrames,validateRange};
