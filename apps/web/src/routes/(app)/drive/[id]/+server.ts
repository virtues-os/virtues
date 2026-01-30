import type { RequestHandler } from './$types';

/**
 * Serve drive files by ID
 * URL pattern: /drive/{file_id}
 *
 * This proxies to the backend API at /api/drive/files/{fileId}/download
 * Enables direct file URLs like /drive/file_abc123 for use in markdown embeds
 */
export const GET: RequestHandler = async ({ params, fetch }) => {
	const { id } = params;

	// Proxy to the backend download API
	const response = await fetch(`/api/drive/files/${id}/download`);

	if (!response.ok) {
		return new Response('File not found', { status: response.status });
	}

	// Get content type and other headers from the response
	const contentType = response.headers.get('content-type') || 'application/octet-stream';
	const contentDisposition = response.headers.get('content-disposition');

	// Return the file with appropriate headers
	const headers = new Headers({
		'Content-Type': contentType,
		'Cache-Control': 'public, max-age=31536000, immutable', // Files are immutable by ID
	});

	if (contentDisposition) {
		headers.set('Content-Disposition', contentDisposition);
	}

	return new Response(response.body, {
		status: 200,
		headers,
	});
};
