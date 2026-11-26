/// Resultado de subir un fichero.
typedef UploadResult = ({int status, String body});

void removeSplash() {}

String browserTimeZone() => 'Europe/Madrid';

String origin() => 'http://localhost:8090';

void redirectTo(String url) {}

void download(String url) {}

Future<UploadResult?> pickAndUpload(String url, String accept) async => null;
