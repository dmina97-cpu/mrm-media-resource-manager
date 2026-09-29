const ALL=['audio','image','video'];
function types(lib){return Array.isArray(lib.types)?ALL.filter(t=>lib.types.includes(t)):ALL.slice();}
function validateTypes(input){if(!Array.isArray(input)||input.some(t=>!ALL.includes(t)))throw Error('Loại tài nguyên không hợp lệ.');const result=ALL.filter(t=>input.includes(t));if(!result.length)throw Error('Chọn ít nhất một loại tài nguyên.');return result;}
module.exports={types,validateTypes,ALL};
