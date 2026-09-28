/**
 * The land the location step's globe is drawn from, as points on a sphere.
 *
 * Natural Earth's 1:50m land polygons (public domain, by way of the
 * world-atlas package, ISC), sampled once offline onto rows one degree
 * apart. Each row holds as many points as its circumference allows, so the
 * dots sit evenly over the whole sphere rather than bunching at the poles,
 * and odd rows are offset half a step so the pattern reads as a weave, not
 * a grid. Only the points that fall on land are kept: about 11,750 of them
 * in 4 KB, bundled with the app. Nothing is fetched, no tiles, no third
 * party; the globe works offline and inside the apps' strict CSP.
 *
 * Encoding: rows separated by ';', from 84°N southward in STEP degrees.
 * Each row is a comma list of base-36 [start, length] runs of land indices.
 */
export const STEP = 1;
export const NORTH = 84;

const ROWS = ';d,1,g,3;d,5,j,4;d,b,p,1,10,2,16,1;e,6,l,7,z,2;e,1,g,4,l,a,11,1,1h,1;k,2,n,b,15,2;d,1,l,2,p,b,1r,2;f,2,j,1,t,a,1n,1,1u,5,25,1;h,2,n,3,w,9,1p,1,1x,7,2a,1;g,2,o,1,z,9,1t,1,22,6;g,3,l,1,n,1,p,2,s,2,10,a,1w,1,24,b,2g,2;h,7,p,1,s,1,u,3,14,9,20,1,25,1,28,g,2r,3;7,1,k,5,r,1,t,7,16,8,1v,1,24,1,28,5,2e,m,31,1;6,6,h,2,m,6,t,2,x,1,z,3,17,1,19,9,1w,4,2a,1,2d,w,3b,2;6,9,g,6,o,2,r,3,v,2,y,1,11,3,1a,9,1z,7,2e,2,2h,2,2k,x,3i,3;0,2,6,j,s,1,v,6,13,1,15,2,1c,8,22,9,2f,8,2o,13;0,3,7,w,16,4,1d,8,24,b,2h,1g;0,4,6,x,17,3,1b,1,1g,7,1s,1,1u,2,27,9,2i,1k;2,1,6,x,14,1,17,5,1i,5,1u,4,29,4,2f,4,2l,1n;9,t,13,5,1c,3,1l,5,1y,2,2d,4,2i,5,2o,1q;7,x,1d,3,1n,4,2d,5,2j,20;7,y,1b,4,1q,3,2f,6,2n,20;7,7,g,q,1e,3,1s,3,2i,6,2q,1t,4l,1,4n,4;7,7,f,1,k,n,1g,3,1w,1,2l,7,2y,1q,4r,1;9,5,m,m,1g,5,1n,1,2o,1,2q,4,2x,1o,4q,1,4v,1;c,2,p,m,1j,4,1o,3,2k,2,2u,3,31,1q,50,2;c,1,o,1,q,n,1k,9,2n,2,2v,1,2x,2,32,1s,53,4;b,1,r,p,1m,9,2q,2,2y,1,30,1,35,1t,58,3;t,r,1m,b,2q,4,30,1,37,1t,5c,4;s,1,u,s,1o,d,2s,3,2x,1,34,25,5a,1,5i,3;v,t,1p,e,2v,2,2y,3,34,29,5e,1,5n,1;w,19,2x,2,30,4,36,2e,5s,1;x,17,33,3,37,2f,5n,1;y,2,11,10,27,1,39,2h,5r,1;10,1,12,z,22,2,28,3,39,2l,5v,1;12,10,23,2,29,5,3a,2o;12,10,23,4,2c,1,2e,1,3e,r,46,1w,64,1;13,16,2b,1,3h,m,45,1,48,7,4i,1n;14,14,29,3,3j,9,3u,a,47,2,4a,7,4k,1o;15,13,2a,1,3l,7,3t,2,3x,a,4e,6,4n,1p,6g,2;15,13,3h,9,3v,3,40,8,4h,5,4p,1o,6i,3;16,14,3k,9,3x,1,3z,3,45,6,4f,2,4m,4,4v,1k;16,12,29,1,3l,8,43,2,47,6,4e,8,4n,6,4x,1l;17,13,3o,7,3x,1,41,1,48,1,4a,3,4f,h,4z,1f,6g,5,6u,1;18,10,3p,7,4c,2,4g,i,52,1e,6i,1,6l,2,6x,1;1a,10,3s,6,48,3,4f,1,4k,h,55,1f,6p,3,70,2;1a,11,3v,3,47,1,4b,1,4h,1,4n,h,57,1j,6t,3,71,4;1c,10,42,9,4o,1,4v,1w,6w,3,74,4;1d,z,3y,e,4u,1,4x,1w,6z,2,74,6;1f,y,40,e,4z,1y,77,3;1h,v,40,h,50,20,78,3;1i,u,41,l,4q,4,52,22,7b,1;1j,1,1l,s,42,o,4s,7,50,27;1k,1,1n,l,2a,1,2c,2,44,1f,5k,1r;1l,1,1n,f,2d,2,41,1,44,1g,5m,1q;1n,1,1p,d,2e,2,44,16,5b,c,5p,1p;1n,2,1q,c,2f,2,45,16,5d,c,5s,2,5v,1l;1p,2,1s,b,2h,1,45,18,5g,d,5y,1k;1p,2,1t,a,46,19,5h,d,5x,1,67,1c;1s,1,1v,9,47,1b,5k,c,5x,4,6b,1a,7n,1;1s,1,1w,8,2h,2,47,1b,5l,j,6d,18,7o,1;j,1,1x,8,2h,1,2l,2,48,1e,5o,j,6g,2,6j,g,71,j,7r,1;1y,7,2c,3,2n,2,2r,1,48,1f,5p,i,6i,1,6k,d,72,e,7i,1;n,1,1y,9,2d,2,2p,2,4a,1e,5r,h,6m,d,75,c,7l,1;20,7,2c,3,2t,4,4b,1e,5t,g,6n,b,77,b,7l,1;22,e,2q,1,2v,1,30,1,4c,1g,5v,e,6p,a,79,c,7y,2;24,c,4c,1h,5x,b,6q,9,7a,c,7z,2;27,3,2b,5,47,1,4e,1h,5z,9,6s,7,7c,1,7f,b,81,1;2c,a,4d,1k,5z,8,6u,5,7h,a,82,1;2e,8,4f,1k,61,4,6v,6,7i,b,84,1;2i,4,4f,1m,62,2,6w,6,7k,1,7m,8,85,1,87,2;2k,3,2y,1,4h,1m,6y,5,7l,1,7p,6,89,2;2k,2,2w,2,2z,2,35,1,4i,1l,67,4,6z,4,7l,1,7q,5,88,1;2l,3,2v,4,30,8,4k,1s,71,3,7n,1,7u,1,8b,3;2n,2,2q,1,2s,1,2v,f,4l,1r,72,1,75,1,7n,2,86,1,8e,1;2q,1,2t,1,2v,h,4m,1q,75,2,7o,2,8b,5;2t,j,4n,1o,76,2,7j,1,7p,1,8b,1,8d,3;2v,k,4p,c,54,18,77,1,7r,2,87,1,8f,1;2u,p,4q,4,4x,1,55,17,7n,2,7s,3,87,2;2v,q,5a,11,7o,2,7s,3,85,4;2u,r,5a,11,7p,3,7t,2,84,6;2u,s,5a,10,7r,3,7v,2,84,6;2s,u,5a,y,7p,1,7r,4,7w,1,81,a,8d,1,8g,1,8j,2;2s,t,3m,1,5a,x,7s,4,82,8,8c,1;2r,w,3o,1,59,x,7s,4,81,8,8b,1,8d,2,8n,3;2s,10,5a,v,7t,4,7y,1,83,6,8c,2,8m,1,8p,2,8u,2,93,1;2s,z,3s,4,5a,u,7t,6,84,4,8b,1,8d,1,8k,2,8o,3,8s,6,97,1;2r,17,5b,t,7u,4,86,1,8b,1,8d,1,8o,1,8q,a;2r,18,5b,s,7v,2,8s,9,96,1;2r,1a,5c,r,8t,9,94,2,99,1;2r,19,5b,r,7w,8,8s,8;2s,18,5c,q,80,4,8r,5,8z,2;2s,17,5b,q,85,1,8d,2,8t,3,8z,2;2t,15,5b,q,88,1,8c,1,90,2;2s,14,5a,q,8i,1;2t,13,5a,q,8i,2,8r,1;2t,11,58,r,67,1,8e,7,8q,1;8,1,2t,10,57,r,65,3,8a,1,8d,7,8o,2;2t,z,56,r,64,3,87,a,8n,3;2u,y,55,r,61,4,85,c,8l,4;2v,w,53,q,5y,5,81,g,8j,4;2w,u,53,o,5x,5,80,m,9g,1;2w,s,52,m,5w,4,7x,n,9e,1;2w,s,52,k,5v,4,7u,r;2v,r,51,k,5t,4,7o,v;2u,r,51,j,5s,4,7m,w;2t,p,4z,j,5p,4,7i,y;2t,m,4y,j,5p,3,7h,y;2r,l,4w,i,5n,3,7e,z;2r,k,4w,g,7d,z;2p,k,4u,g,7a,z;2p,k,4t,f,78,z;2n,j,4s,d,76,y;2n,i,4r,c,74,x;2l,h,4q,a,71,x;2k,h,4o,a,6z,b,7e,g;2j,g,4m,8,6w,7,7c,2,7f,b;2i,f,4l,6,6t,4,79,1,7b,c;2g,c,6r,1,79,a,81,1;2f,d,74,1,77,8,7y,1;2d,e,74,8,7w,1;2d,c,71,6,7s,3;2b,9,7n,3;2a,9,7l,1;28,7,2g,1,6t,1,6v,1,7e,1,7g,1;27,1,29,5,6r,2,7a,2;25,7,6m,2,74,2;25,6,6z,3;24,5,6u,3;21,6,6p,3;20,5;20,5;1w,6;1x,3;1u,4;1t,1,1v,2;1u,1;1t,2;1q,1,1s,1;;;;;;;1l,1;;1h,1;1d,1;1b,1,2n,1,37,1,3b,1;18,1,2i,3,2x,b,39,3,3d,6;2c,a,2q,p;14,2,24,1,27,a,2k,s;10,2,13,2,1v,1,21,c,2f,u;z,1,11,2,1k,1,1n,m,2a,v;o,2,y,3,1h,1j;m,1,q,1,x,2,1c,1i;f,2,i,1,m,b,1a,1d;b,7,l,9,16,1a;9,j,12,19;8,f,x,18;5,g,m,1,s,1,v,15;3,1,5,e,p,1,s,12;6,c,k,2,n,1,s,w;4,d,m,v;4,c,i,1,k,s;3,13;2,10;;';

/** Every land point as [lat, lng] in degrees, flattened: lat0, lng0, lat1, lng1, ... */
export function landPoints(): Float32Array {
	const rows = ROWS.split(';');
	const out: number[] = [];
	rows.forEach((row, i) => {
		if (!row) return;
		const lat = NORTH - i * STEP;
		const n = Math.max(1, Math.round((360 * Math.cos((lat * Math.PI) / 180)) / STEP));
		const off = (i % 2) * 0.5;
		const runs = row.split(',').map((v) => parseInt(v, 36));
		for (let k = 0; k < runs.length; k += 2) {
			for (let j = runs[k]; j < runs[k] + runs[k + 1]; j++) out.push(lat, -180 + ((j + off) * 360) / n);
		}
	});
	return new Float32Array(out);
}
