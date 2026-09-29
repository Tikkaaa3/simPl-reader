# Yerel sözlük ve çeviri seçenekleri — 30 Eylül 2026

Bu belge ilk araştırma ve tasarım önerisidir. Sonraki uygulamada yerel kelime
sözlükleri eklendi; kapsam ve kaynaklar [veri bildiriminde](assets/dictionaries/README.md)
ve [README'de](README.md) yer alıyor. Argos/model/eklenti altyapısı eklenmedi.
Veriler/modeller bir kez dosyadan kurulunca kullanım
çevrimdışı olabilir; internetten paket edinmek ayrı bir işlemdir. Aşağıdaki
karşılaştırma bir çeviri kalitesi veya Windows performans benchmark'ı değildir.

## Hafif sözlük verileri

| Kaynak | Kapsam ve format | simPl açısından değerlendirme |
| --- | --- | --- |
| [WikDict](https://www.wikdict.com/page/download) | Wiktionary tabanlı; SQLite, StarDict, TEI; CC BY-SA 4.0 | Hazır SQLite verisiyle ilk kelime karşılığı için güçlü aday. Kelime türü/çekim verileri ayrı dil veritabanlarında; iki yöndeki dosyalar birbirinin aynısı değil. |
| [FreeDict](https://freedict.org/downloads/) | TEI, dictd, StarDict vb.; dil çiftine göre kapsam/lisans değişir | Alternatif ve tamamlayıcı kaynak. İngilizce→Türkçe katalogda 36.589 başlık. [eng-tur kaynağı](https://github.com/freedict/fd-dictionaries/blob/master/eng-tur/eng-tur.tei) GPL-2.0-or-later; veri güncelliği/kalitesi çift bazında kontrol edilmeli. |
| [JMdict/EDICT](https://www.edrdg.org/wiki/JMdict-EDICT_Dictionary_Project.html) | Japonca başlıklar, okunuşlar, anlamlar; XML/metin | Japonca→İngilizce için özel aday. [EDRDG lisansı](https://www.edrdg.org/edrdg/licence.html) CC BY-SA 4.0; kaynak ve lisans bildirimi gerekli. İngilizce→Japonca için ayrı ters arama indeksi gerekir. |
| [CC-CEDICT](https://www.mdbg.net/chinese/dictionary?page=cedict) | Mandarin→İngilizce; geleneksel/basitleştirilmiş karakterler ve pinyin; metin | Çince için özel aday. CC BY-SA 4.0. İncelenen sayfa 125.139 kayıt bildiriyor. İngilizce ters arama mümkün olsa da iki bağımsız sözlükle aynı kapsamı vaat etmez. |
| [Kaikki / Wiktextract](https://kaikki.org/dictionary/) | Wiktionary'den JSONL; yüzlerce dil, anlamlar ve dilbilgisi bilgileri; CC BY-SA/GFDL | Korece dahil genel altyapı. Hazır son kullanıcı paketi yerine derleme sırasında süzülüp indekslenmesi daha uygun. |
| [Korean-English Learners' Dictionary](https://krdict.korean.go.kr/eng/mainAction) | Kore Ulusal Dil Enstitüsü'nün öğrenici sözlüğü | Korece için ikinci aday. [Metin politikası](https://krdict.korean.go.kr/eng/kboardPolicy/copyRightTermsInfo) CC BY-SA; medya lisansları ayrıca değişebilir. Eksiksiz İngilizce dışa aktarımın kapsamı/edinme süreci ayrıca doğrulanmalı. Online API, tek başına yerel çözüm değildir. |

WikDict'in [güncel SQLite kataloğundaki](https://download.wikdict.com/dictionaries/sqlite/2/)
yaklaşık dosya boyutları aşağıda. Bunlar yalnızca çeviri çiftinin dosyalarıdır;
çekim/kelime türü için ayrı dil veritabanları eklenirse toplam büyür. Boyut, anlam
kapsamını veya kaliteyi ölçmez.

| Yön | SQLite dosyası | İkinci yön |
| --- | ---: | ---: |
| İngilizce→Türkçe | 8 MB | Türkçe→İngilizce: 4 MB |
| İspanyolca→İngilizce | 11 MB | İngilizce→İspanyolca: 15 MB |
| Almanca→İngilizce | 25 MB | İngilizce→Almanca: 20 MB |
| Fransızca→İngilizce | 22 MB | İngilizce→Fransızca: 23 MB |
| Japonca→İngilizce | 5 MB | İngilizce→Japonca: 9 MB |
| Çince→İngilizce | 12 MB | İngilizce→Çince: 5 MB |
| Korece→İngilizce | İncelenen WikDict kataloğunda yok | Kaikki / öğrenici sözlüğü değerlendirilmeli |

Araştırma sırasında İngilizce→Türkçe SQLite dosyası indirildi ve salt okunur
sorgulandı: 8.417.280 bayt, `simple_translation` tablosunda 47.626 satır.
`book` ve `read` karşılık verdi; `ran` doğrudan bulunmadı. `lead` için yalnızca
metal anlamı geldi. Bu küçük örnek, çekimli sözcükleri köke götürme ve çok anlamlı
kelimeler için kapsam kontrolünün gerekli olduğunu gösteriyor; tam kalite testi değil.
Dosya ve örnek sorgular `target/dictionary-research/` içinde, uygulamaya dahil değil.

[Korece Kaikki sayfası](https://kaikki.org/dictionary/Korean/index.html) İngilizce
Wiktionary'den 57.252 farklı biçim ve 195,4 MB işlenmiş JSONL bildiriyor; bunlara
karakterler/özel adlar da dahil. Bu işlenmiş indirme kullanımdan kaldırılacak.
[Önerilen ham kaynağın](https://kaikki.org/dictionary/rawdata.html) İngilizce
sürümü 2,8 GB sıkıştırılmış / 23,9 GB açık; paket üretiminde `lang_code=ko` süzülmeli.
Sayfadaki `ko-extract` ise Korece Wiktionary sürümüdür; İngilizce açıklamalı Korece
paketi yerine yanlışlıkla o dosya seçilmemeli. Son kullanıcının bütün dump'ı
indirmesi gerekmez; bizim hazırlayacağımız süzülmüş paketin boyutu henüz ölçülmedi.

## Yerel cümle/paragraf çevirisi

| Seçenek | Kapsam/lisans | Değerlendirme |
| --- | --- | --- |
| [Argos Translate](https://github.com/argosopentech/argos-translate) | Yerel Python motoru, ayrı `.argosmodel` paketleri; MIT/CC0 | İlk isteğe bağlı çeviri motoru adayı. Eksik doğrudan çiftlerde ara dil kullanabilir; ek kalite kaybı olabilir. Python/native bağımlılıklarının Windows paketlemesi ayrıca denenmeli. |
| [OPUS-MT / Marian](https://github.com/Helsinki-NLP/Opus-MT) | İndirilebilir dil modelleri; proje kodu MIT, belirtilen OPUS-MT modelleri CC BY 4.0 | Dil çiftine özel modeller seçilebilir. Argos'a göre tokenizer/model/runtime entegrasyonunu daha çok kendimiz üstleniriz. Her checkpoint'in kartı ayrıca kontrol edilmeli. |
| [Bergamot / translateLocally](https://github.com/XapaJIaMnu/translateLocally) | C++ tabanlı yerel motor; Windows uygulaması ve CLI; uygulama MIT | CPU odaklı alternatif. Katalogdaki modellerle sınırlı; tüm hedef çiftlerin varlığı doğrulanmadı. Motor, arayüz ve model lisansları ayrı kontrol edilmeli. |
| [Apertium](https://github.com/apertium/apertium) | Kural tabanlı çeviri; çekirdek GPL-2.0 | [Dil çiftleri](https://www.wiki.apertium.org/wiki/List_of_language_pairs) olgunluk/yön bakımından değişiyor. Bazı Avrupa dillerinde aday; istenen yedi çift için ortak başlangıç çözümü olarak önermiyorum. |
| [M2M100 418M](https://huggingface.co/facebook/m2m100_418M) | 100 dil, hedef yedi dil dahil; MIT | Çok sayıda çifti tek modelle kapsama adayı. 418 milyon parametre; sözlük paketinden daha ağır. Gerçek Windows hız/RAM/kalite ölçülmedi. |
| [NLLB-200 600M](https://huggingface.co/facebook/nllb-200-distilled-600M) | Geniş dil kapsamı; CC BY-NC 4.0 | Yerel araştırma için seçenek; ticari kullanımı sınırlayan lisansı nedeniyle varsayılan dağıtım adayı olarak önermiyorum. Model kartı araştırma kullanımını öne çıkarıyor. |
| [MADLAD-400 3B](https://huggingface.co/google/madlad400-3b-mt) | 400+ dil; Apache-2.0 | Geniş kapsamlı, isteğe bağlı büyük model adayı. Üç milyar parametre ve ek çalışma ortamı hafif okuyucu paketini belirgin büyütür. |
| [TranslateGemma](https://blog.google/innovation-and-ai/technology/developers-tools/translategemma/) | 55 dil; 4B, 12B, 27B | Yeni büyük model alternatifi. [Model kartı](https://huggingface.co/google/translategemma-4b-it) Gemma kullanım şartları ve indirme için lisans kabulü belirtiyor. Hafif ilk sürüm için fazla büyük; kullanıcı tercihiyle ayrı motor olabilir. |
| [LibreTranslate](https://github.com/LibreTranslate/LibreTranslate) | Yerelde kurulabilen HTTP servisi; Argos tabanlı; AGPL-3.0 | Yeni bir çeviri modeli değil. Yerel servis kullanan kullanıcıya bağlanma seçeneği; simPl için başlangıçta ekstra servis kurdurmak yerine doğrudan motor daha sade olur. |

Argos'un [30 Eylül'de okunan resmi paket indeksi](https://raw.githubusercontent.com/argosopentech/argospm-index/main/index.json)
Türkçe, İspanyolca, Almanca, Fransızca, Japonca, Korece ve Çince için İngilizceyle
iki yönde doğrudan paket içeriyor. Geleneksel Çince için ayrıca `zt` paketleri var.
Türkçe paket sürümü 1.5; Japonca/Korece 1.1; bunlar motorun güncel sürümü veya kalite
puanı değildir. Bakımcı [model dosyalarının da MIT/CC0 olduğunu açıklamış](https://github.com/argosopentech/argos-translate/issues/533#issuecomment-5160080718).
Paket URL'lerine yapılan HEAD sorguları 403 döndü; bu oturumda model indirme boyutları
ve indirme/çalıştırma başarısı doğrulanmadı. Hiçbir çeviri motoru kurulup çalıştırılmadı.

## Dosya formatları ve hazır uygulamalar

[GoldenDict](https://github.com/goldendict/goldendict) çok sayıda yerel sözlük
formatını okuyor; kullanıcı isterse ilk aşamada harici sözlük uygulaması olarak
değerlendirilebilir. Bu, simPl içindeki anlam panelinin yerini tamamen tutmaz.
[Yomitan](https://github.com/yomidevs/yomitan) açılır sözlük/dil öğrenme arayüzü için
örnek; bir tarayıcı eklentisidir. [PyGlossary](https://github.com/ilius/pyglossary)
paket üretiminde format dönüştürme adayıdır. StarDict/MDict/Yomitan birer dosya
formatı veya ekosistemdir; dosyanın formatı, içindeki sözlük verisinin lisansını belirlemez.

## simPl için öneri

Önce kelime/ifade araması ve sonradan eklenebilen veri paketleri. İlk paketler
WikDict İngilizce→Türkçe, İspanyolca→İngilizce, Almanca→İngilizce,
Fransızca→İngilizce olabilir. Japonca JMdict, Çince CC-CEDICT, Korece süzülmüş
Kaikki verisiyle ayrı veri adaptörleri üzerinden eklenebilir. Bunlar bir öneri;
başlangıç önerisiydi. Kelime araması şimdi 13 yönü kapsayan gömülü ve çevrimdışı
sözlüklerle uygulandı; isteğe bağlı paket indirme henüz uygulanmadı.

İlk sürüm için DLL çalıştıran genel eklenti sistemi yerine, kaynak/hedef dil,
sürüm, kaynak bağlantısı, lisans ve hash içeren bir bildirim ile salt okunur
indeks dosyası yeterli olabilir. Kullanıcı istediği dil paketini seçer; ana
kurulum tüm dilleri taşımaz. Çekimden köke erişim, Japonca/Çince sözcük sınırları
ve Korece ekler, sadece eşit metin aramasıyla çözülemeyecek ayrı ürün işleri.

Sonra isteğe bağlı cümle çevirisi: aynı panelde ayrı çeviri sağlayıcısı,
öncelikle Argos ve karşılaştırma için OPUS-MT/Bergamot. Modeller yalnızca
gerektiğinde yüklenmeli, iş UI iş parçacığını durdurmamalı, yeni seçim önceki işi
iptal etmeli. Daha büyük M2M100/MADLAD/TranslateGemma modelleri tercihe bağlı kalmalı.

Karardan önce her hedef dil için ortak bir küçük değerlendirme setiyle nadir
kelimeler, çok anlamlılık, çekimler, deyimler ve birkaç kısa paragraf denenmeli.
Windows'ta ilk yükleme, tekrar arama, RAM ve CPU ölçülmeli. Bugün doğrulanan
katalog kapsamıdır; kalite, sözlük indeksleme performansı ve model performansı açık.

## Uygulanan sözlüklerin dağıtım boyutu — 30 Eylül 2026

Kaynak SQLite/JSONL boyutları son kullanıcıya gönderilen boyut değildir.
`assets/dictionaries/words.zip` içindeki hazırlanmış, sıkıştırılmış indeksler
toplam **19.659.286 bayt (19,66 MB / 18,75 MiB)**. Aşağıdaki MB değerleri
ondalık; arşiv girdilerinin sıkıştırılmış boyutları toplandı. Ayrı paketlerin
manifest ve lisans dosyaları küçük bir ek yük getirir.

| Dil paketi | Sıkıştırılmış veri | Açık TSV veri |
| --- | ---: | ---: |
| İngilizce ↔ Türkçe | 1,11 MB | 3,22 MB |
| İngilizce ↔ İspanyolca | 2,20 MB | 7,07 MB |
| İngilizce ↔ Almanca | 3,63 MB | 11,30 MB |
| İngilizce ↔ Fransızca | 4,00 MB | 13,67 MB |
| İngilizce ↔ Japonca | 1,56 MB | 4,31 MB |
| İngilizce ↔ Çince | 5,89 MB | 16,40 MB |
| Korece → İngilizce | 1,26 MB | 3,36 MB |

Mevcut uygulama tüm arşivi exe içinde taşır; ZIP'in kendisi ayrıca kuruluma
kopyalanmaz. Yalnızca etkin yön açılır ve bellekte indekslenir. Açık TSV boyutu
RAM tüketiminin tamamı değildir: dizeler ve arama indeksi de yer kaplar.
Argos, Python, SQLite runtime veya çeviri modeli bu pakete dahil değildir.

Bu dalda hazırlanan Windows x64 build ölçümleri: exe **38.008.320 bayt
(38,01 MB)**; PDFium dahil portable klasör **47.591.520 bayt (47,59 MB)**;
DEFLATE seviye 9 portable ZIP **31.489.574 bayt (31,49 MB)**. ZIP CRC kontrolü
geçti. Bunlar installer boyutu değildir; setup yeniden hazırlanmadı.

Dağıtım seçenekleri:

- **Hepsi gömülü:** sözlükler ilk açılıştan itibaren çevrimdışı çalışır;
  yaklaşık 19,7 MB veri ana uygulamaya ve her uygulama güncellemesine eklenir.
- **Öneri — İngilizce ↔ Türkçe gömülü, diğerleri isteğe bağlı:** varsayılan
  sözlük hazır kalır; gömülü veri yaklaşık 1,1 MB olur. Diğer dil paketleri
  bir kez indirilir ve yerelde saklanır. Ana exe'den yaklaşık 18,5 MB veri çıkar.
- **Tüm sözlükler ayrı:** ana uygulamada sözlük verisi bulunmaz; ilk kullanımda
  bir paket indirme veya dosyadan ekleme gerekir. İnternetsiz kurulum için
  ayrıca tüm paketleri içeren bir portable dağıtım sunulabilir.

İndirme kullanıcı tarafından başlatılmalı; paketler dil yönü, sürüm, SHA-256,
kaynak ve lisans bildirimini taşımalı. Önerilen saklama yeri
`%LOCALAPPDATA%\simPl\dictionaries`; çevrimdışı dosyadan ekleme de aynı
doğrulama yolunu kullanabilir. Dil paketleri uygulama güncellemelerinden
bağımsız yenilenebilir. Bunlar salt okunur sözlük verileridir; Argos sağlayıcısı
daha sonra ayrı bir çalışma motoru ve model paketi olarak ele alınmalı.

Bu bölüm bir dağıtım değerlendirmesidir; indirme sistemi veya sözlüklerin
exe'den çıkarılması bu değişiklikte uygulanmadı.
