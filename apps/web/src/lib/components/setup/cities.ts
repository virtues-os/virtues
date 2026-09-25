/**
 * The cities the location step's map is drawn from — the map IS these dots.
 *
 * No tiles and no coastline data: a world of lit cities on a graticule reads
 * as the world, needs nothing fetched from anyone, and carries exactly the
 * detail the step wants (cities, never streets or shops). Each city carries
 * its real IANA zone, so dropping the pin near one picks a true zone rather
 * than an offset band's guess.
 *
 * Chosen for coverage, not rank: every inhabited continent densely enough to
 * suggest its shape, and at least one city in each zone people commonly
 * live in. Public facts only — name, country, rounded coordinates, zone.
 */
export interface City {
	name: string;
	country: string;
	lat: number;
	lng: number;
	zone: string;
}

type Row = [string, string, number, number, string];

const ROWS: Row[] = [
	// North America
	['Anchorage', 'United States', 61.2, -149.9, 'America/Anchorage'],
	['Honolulu', 'United States', 21.3, -157.9, 'Pacific/Honolulu'],
	['Vancouver', 'Canada', 49.3, -123.1, 'America/Vancouver'],
	['Seattle', 'United States', 47.6, -122.3, 'America/Los_Angeles'],
	['Portland', 'United States', 45.5, -122.7, 'America/Los_Angeles'],
	['San Francisco', 'United States', 37.8, -122.4, 'America/Los_Angeles'],
	['Los Angeles', 'United States', 34.1, -118.2, 'America/Los_Angeles'],
	['San Diego', 'United States', 32.7, -117.2, 'America/Los_Angeles'],
	['Las Vegas', 'United States', 36.2, -115.1, 'America/Los_Angeles'],
	['Phoenix', 'United States', 33.4, -112.1, 'America/Phoenix'],
	['Salt Lake City', 'United States', 40.8, -111.9, 'America/Denver'],
	['Denver', 'United States', 39.7, -105.0, 'America/Denver'],
	['Calgary', 'Canada', 51.0, -114.1, 'America/Edmonton'],
	['Edmonton', 'Canada', 53.5, -113.5, 'America/Edmonton'],
	['Winnipeg', 'Canada', 49.9, -97.1, 'America/Winnipeg'],
	['Minneapolis', 'United States', 45.0, -93.3, 'America/Chicago'],
	['Chicago', 'United States', 41.9, -87.6, 'America/Chicago'],
	['St. Louis', 'United States', 38.6, -90.2, 'America/Chicago'],
	['Kansas City', 'United States', 39.1, -94.6, 'America/Chicago'],
	['Dallas', 'United States', 32.8, -96.8, 'America/Chicago'],
	['Austin', 'United States', 30.3, -97.7, 'America/Chicago'],
	['Houston', 'United States', 29.8, -95.4, 'America/Chicago'],
	['New Orleans', 'United States', 30.0, -90.1, 'America/Chicago'],
	['Nashville', 'United States', 36.2, -86.8, 'America/Chicago'],
	['Detroit', 'United States', 42.3, -83.0, 'America/Detroit'],
	['Atlanta', 'United States', 33.7, -84.4, 'America/New_York'],
	['Miami', 'United States', 25.8, -80.2, 'America/New_York'],
	['Charlotte', 'United States', 35.2, -80.8, 'America/New_York'],
	['Washington', 'United States', 38.9, -77.0, 'America/New_York'],
	['Philadelphia', 'United States', 40.0, -75.2, 'America/New_York'],
	['New York', 'United States', 40.7, -74.0, 'America/New_York'],
	['Boston', 'United States', 42.4, -71.1, 'America/New_York'],
	['Toronto', 'Canada', 43.7, -79.4, 'America/Toronto'],
	['Montreal', 'Canada', 45.5, -73.6, 'America/Toronto'],
	['Halifax', 'Canada', 44.6, -63.6, 'America/Halifax'],
	["St. John's", 'Canada', 47.6, -52.7, 'America/St_Johns'],
	['Monterrey', 'Mexico', 25.7, -100.3, 'America/Monterrey'],
	['Guadalajara', 'Mexico', 20.7, -103.3, 'America/Mexico_City'],
	['Mexico City', 'Mexico', 19.4, -99.1, 'America/Mexico_City'],
	['Guatemala City', 'Guatemala', 14.6, -90.5, 'America/Guatemala'],
	['San José', 'Costa Rica', 9.9, -84.1, 'America/Costa_Rica'],
	['Panama City', 'Panama', 9.0, -79.5, 'America/Panama'],
	['Havana', 'Cuba', 23.1, -82.4, 'America/Havana'],
	['San Juan', 'Puerto Rico', 18.5, -66.1, 'America/Puerto_Rico'],
	// South America
	['Bogotá', 'Colombia', 4.7, -74.1, 'America/Bogota'],
	['Medellín', 'Colombia', 6.2, -75.6, 'America/Bogota'],
	['Caracas', 'Venezuela', 10.5, -66.9, 'America/Caracas'],
	['Quito', 'Ecuador', -0.2, -78.5, 'America/Guayaquil'],
	['Lima', 'Peru', -12.0, -77.0, 'America/Lima'],
	['La Paz', 'Bolivia', -16.5, -68.1, 'America/La_Paz'],
	['Manaus', 'Brazil', -3.1, -60.0, 'America/Manaus'],
	['Recife', 'Brazil', -8.1, -34.9, 'America/Recife'],
	['Salvador', 'Brazil', -13.0, -38.5, 'America/Bahia'],
	['Brasília', 'Brazil', -15.8, -47.9, 'America/Sao_Paulo'],
	['Rio de Janeiro', 'Brazil', -22.9, -43.2, 'America/Sao_Paulo'],
	['São Paulo', 'Brazil', -23.6, -46.6, 'America/Sao_Paulo'],
	['Asunción', 'Paraguay', -25.3, -57.6, 'America/Asuncion'],
	['Montevideo', 'Uruguay', -34.9, -56.2, 'America/Montevideo'],
	['Buenos Aires', 'Argentina', -34.6, -58.4, 'America/Argentina/Buenos_Aires'],
	['Córdoba', 'Argentina', -31.4, -64.2, 'America/Argentina/Cordoba'],
	['Santiago', 'Chile', -33.4, -70.7, 'America/Santiago'],
	['Punta Arenas', 'Chile', -53.2, -70.9, 'America/Punta_Arenas'],
	// Europe
	['Reykjavík', 'Iceland', 64.1, -21.9, 'Atlantic/Reykjavik'],
	['Lisbon', 'Portugal', 38.7, -9.1, 'Europe/Lisbon'],
	['Dublin', 'Ireland', 53.3, -6.3, 'Europe/Dublin'],
	['Edinburgh', 'United Kingdom', 55.95, -3.2, 'Europe/London'],
	['Manchester', 'United Kingdom', 53.5, -2.2, 'Europe/London'],
	['London', 'United Kingdom', 51.5, -0.1, 'Europe/London'],
	['Madrid', 'Spain', 40.4, -3.7, 'Europe/Madrid'],
	['Barcelona', 'Spain', 41.4, 2.2, 'Europe/Madrid'],
	['Seville', 'Spain', 37.4, -6.0, 'Europe/Madrid'],
	['Paris', 'France', 48.9, 2.35, 'Europe/Paris'],
	['Lyon', 'France', 45.8, 4.8, 'Europe/Paris'],
	['Marseille', 'France', 43.3, 5.4, 'Europe/Paris'],
	['Brussels', 'Belgium', 50.8, 4.35, 'Europe/Brussels'],
	['Amsterdam', 'Netherlands', 52.4, 4.9, 'Europe/Amsterdam'],
	['Zurich', 'Switzerland', 47.4, 8.5, 'Europe/Zurich'],
	['Milan', 'Italy', 45.5, 9.2, 'Europe/Rome'],
	['Rome', 'Italy', 41.9, 12.5, 'Europe/Rome'],
	['Naples', 'Italy', 40.85, 14.3, 'Europe/Rome'],
	['Munich', 'Germany', 48.1, 11.6, 'Europe/Berlin'],
	['Frankfurt', 'Germany', 50.1, 8.7, 'Europe/Berlin'],
	['Hamburg', 'Germany', 53.55, 10.0, 'Europe/Berlin'],
	['Berlin', 'Germany', 52.5, 13.4, 'Europe/Berlin'],
	['Copenhagen', 'Denmark', 55.7, 12.6, 'Europe/Copenhagen'],
	['Oslo', 'Norway', 59.9, 10.75, 'Europe/Oslo'],
	['Stockholm', 'Sweden', 59.3, 18.1, 'Europe/Stockholm'],
	['Helsinki', 'Finland', 60.2, 24.9, 'Europe/Helsinki'],
	['Vienna', 'Austria', 48.2, 16.4, 'Europe/Vienna'],
	['Prague', 'Czechia', 50.1, 14.4, 'Europe/Prague'],
	['Warsaw', 'Poland', 52.2, 21.0, 'Europe/Warsaw'],
	['Budapest', 'Hungary', 47.5, 19.0, 'Europe/Budapest'],
	['Belgrade', 'Serbia', 44.8, 20.5, 'Europe/Belgrade'],
	['Athens', 'Greece', 37.98, 23.7, 'Europe/Athens'],
	['Bucharest', 'Romania', 44.4, 26.1, 'Europe/Bucharest'],
	['Kyiv', 'Ukraine', 50.45, 30.5, 'Europe/Kyiv'],
	['Istanbul', 'Türkiye', 41.0, 29.0, 'Europe/Istanbul'],
	['Ankara', 'Türkiye', 39.9, 32.9, 'Europe/Istanbul'],
	['Moscow', 'Russia', 55.75, 37.6, 'Europe/Moscow'],
	['Saint Petersburg', 'Russia', 59.9, 30.3, 'Europe/Moscow'],
	// Africa
	['Casablanca', 'Morocco', 33.6, -7.6, 'Africa/Casablanca'],
	['Algiers', 'Algeria', 36.75, 3.05, 'Africa/Algiers'],
	['Tunis', 'Tunisia', 36.8, 10.2, 'Africa/Tunis'],
	['Cairo', 'Egypt', 30.0, 31.2, 'Africa/Cairo'],
	['Dakar', 'Senegal', 14.7, -17.5, 'Africa/Dakar'],
	['Accra', 'Ghana', 5.6, -0.2, 'Africa/Accra'],
	['Lagos', 'Nigeria', 6.5, 3.4, 'Africa/Lagos'],
	['Kinshasa', 'DR Congo', -4.3, 15.3, 'Africa/Kinshasa'],
	['Khartoum', 'Sudan', 15.6, 32.5, 'Africa/Khartoum'],
	['Addis Ababa', 'Ethiopia', 9.0, 38.75, 'Africa/Addis_Ababa'],
	['Nairobi', 'Kenya', -1.3, 36.8, 'Africa/Nairobi'],
	['Dar es Salaam', 'Tanzania', -6.8, 39.3, 'Africa/Dar_es_Salaam'],
	['Luanda', 'Angola', -8.8, 13.2, 'Africa/Luanda'],
	['Harare', 'Zimbabwe', -17.8, 31.05, 'Africa/Harare'],
	['Johannesburg', 'South Africa', -26.2, 28.05, 'Africa/Johannesburg'],
	['Cape Town', 'South Africa', -33.9, 18.4, 'Africa/Johannesburg'],
	['Antananarivo', 'Madagascar', -18.9, 47.5, 'Indian/Antananarivo'],
	// Middle East and Asia
	['Tel Aviv', 'Israel', 32.1, 34.8, 'Asia/Jerusalem'],
	['Beirut', 'Lebanon', 33.9, 35.5, 'Asia/Beirut'],
	['Baghdad', 'Iraq', 33.3, 44.4, 'Asia/Baghdad'],
	['Riyadh', 'Saudi Arabia', 24.7, 46.7, 'Asia/Riyadh'],
	['Tehran', 'Iran', 35.7, 51.4, 'Asia/Tehran'],
	['Dubai', 'United Arab Emirates', 25.2, 55.3, 'Asia/Dubai'],
	['Doha', 'Qatar', 25.3, 51.5, 'Asia/Qatar'],
	['Tashkent', 'Uzbekistan', 41.3, 69.25, 'Asia/Tashkent'],
	['Almaty', 'Kazakhstan', 43.25, 76.9, 'Asia/Almaty'],
	['Karachi', 'Pakistan', 24.9, 67.0, 'Asia/Karachi'],
	['Lahore', 'Pakistan', 31.55, 74.35, 'Asia/Karachi'],
	['Delhi', 'India', 28.6, 77.2, 'Asia/Kolkata'],
	['Mumbai', 'India', 19.1, 72.9, 'Asia/Kolkata'],
	['Bengaluru', 'India', 12.97, 77.6, 'Asia/Kolkata'],
	['Chennai', 'India', 13.1, 80.3, 'Asia/Kolkata'],
	['Kolkata', 'India', 22.6, 88.4, 'Asia/Kolkata'],
	['Kathmandu', 'Nepal', 27.7, 85.3, 'Asia/Kathmandu'],
	['Colombo', 'Sri Lanka', 6.9, 79.9, 'Asia/Colombo'],
	['Dhaka', 'Bangladesh', 23.8, 90.4, 'Asia/Dhaka'],
	['Yangon', 'Myanmar', 16.8, 96.2, 'Asia/Yangon'],
	['Bangkok', 'Thailand', 13.75, 100.5, 'Asia/Bangkok'],
	['Hanoi', 'Vietnam', 21.0, 105.85, 'Asia/Ho_Chi_Minh'],
	['Ho Chi Minh City', 'Vietnam', 10.8, 106.7, 'Asia/Ho_Chi_Minh'],
	['Kuala Lumpur', 'Malaysia', 3.1, 101.7, 'Asia/Kuala_Lumpur'],
	['Singapore', 'Singapore', 1.35, 103.8, 'Asia/Singapore'],
	['Jakarta', 'Indonesia', -6.2, 106.8, 'Asia/Jakarta'],
	['Manila', 'Philippines', 14.6, 121.0, 'Asia/Manila'],
	['Hong Kong', 'China', 22.3, 114.2, 'Asia/Hong_Kong'],
	['Shenzhen', 'China', 22.5, 114.1, 'Asia/Shanghai'],
	['Chengdu', 'China', 30.7, 104.1, 'Asia/Shanghai'],
	['Shanghai', 'China', 31.2, 121.5, 'Asia/Shanghai'],
	['Beijing', 'China', 39.9, 116.4, 'Asia/Shanghai'],
	['Taipei', 'Taiwan', 25.0, 121.5, 'Asia/Taipei'],
	['Ulaanbaatar', 'Mongolia', 47.9, 106.9, 'Asia/Ulaanbaatar'],
	['Seoul', 'South Korea', 37.6, 127.0, 'Asia/Seoul'],
	['Osaka', 'Japan', 34.7, 135.5, 'Asia/Tokyo'],
	['Tokyo', 'Japan', 35.7, 139.7, 'Asia/Tokyo'],
	['Sapporo', 'Japan', 43.1, 141.35, 'Asia/Tokyo'],
	['Novosibirsk', 'Russia', 55.0, 82.9, 'Asia/Novosibirsk'],
	['Yekaterinburg', 'Russia', 56.8, 60.6, 'Asia/Yekaterinburg'],
	['Irkutsk', 'Russia', 52.3, 104.3, 'Asia/Irkutsk'],
	['Vladivostok', 'Russia', 43.1, 131.9, 'Asia/Vladivostok'],
	// Oceania
	['Perth', 'Australia', -31.95, 115.9, 'Australia/Perth'],
	['Darwin', 'Australia', -12.5, 130.8, 'Australia/Darwin'],
	['Adelaide', 'Australia', -34.9, 138.6, 'Australia/Adelaide'],
	['Brisbane', 'Australia', -27.5, 153.0, 'Australia/Brisbane'],
	['Sydney', 'Australia', -33.9, 151.2, 'Australia/Sydney'],
	['Melbourne', 'Australia', -37.8, 145.0, 'Australia/Melbourne'],
	['Hobart', 'Australia', -42.9, 147.3, 'Australia/Hobart'],
	['Port Moresby', 'Papua New Guinea', -9.4, 147.2, 'Pacific/Port_Moresby'],
	['Nouméa', 'New Caledonia', -22.3, 166.4, 'Pacific/Noumea'],
	['Suva', 'Fiji', -18.1, 178.4, 'Pacific/Fiji'],
	['Auckland', 'New Zealand', -36.85, 174.8, 'Pacific/Auckland'],
	['Wellington', 'New Zealand', -41.3, 174.8, 'Pacific/Auckland'],
	['Christchurch', 'New Zealand', -43.5, 172.6, 'Pacific/Auckland'],
	['Apia', 'Samoa', -13.8, -171.8, 'Pacific/Apia'],
	['Papeete', 'French Polynesia', -17.5, -149.6, 'Pacific/Tahiti'],
];

export const CITIES: City[] = ROWS.map(([name, country, lat, lng, zone]) => ({ name, country, lat, lng, zone }));

/** The nearest city to a point, by great-circle distance. */
export function nearestCity(lat: number, lng: number): City {
	const rad = Math.PI / 180;
	let best = CITIES[0];
	let bestD = Infinity;
	for (const c of CITIES) {
		const dLat = (c.lat - lat) * rad;
		const dLng = (c.lng - lng) * rad;
		const a =
			Math.sin(dLat / 2) ** 2 + Math.cos(lat * rad) * Math.cos(c.lat * rad) * Math.sin(dLng / 2) ** 2;
		if (a < bestD) {
			bestD = a;
			best = c;
		}
	}
	return best;
}

/** The city for a zone, if the list has one ("America/Chicago" → Chicago). */
export function cityForZone(zone: string): City | null {
	const leaf = zone.split('/').pop()!.replace(/_/g, ' ');
	return CITIES.find((c) => c.zone === zone && c.name === leaf) ?? CITIES.find((c) => c.zone === zone) ?? null;
}
