import { createServer } from 'node:http';
createServer((_req, res) => {
 res.writeHead(200, {'content-type':'application/json'});
 res.end(JSON.stringify({status:'ok',http:'up',version:'1.2.3'}));
}).listen(3000,'0.0.0.0');
