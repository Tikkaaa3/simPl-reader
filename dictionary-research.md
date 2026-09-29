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
Kaikki verisiyle ayrı veri adaptörleri üzerinden eklenebilir. Bunlar başlangıç önerisiydi. Kelime araması şimdi 13 yönü kapsayan çevrimdışı
sözlüklerle uygulandı. Kullanıcı kararıyla tüm sözlükler isteğe bağlı indirilebilir
paketlere ayrıldı; ana uygulama kelime verisini taşımıyor.

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

## Uygulanan indirilebilir sözlükler — 30 Eylül 2026

Tüm 13 dil yönü ayrı, isteğe bağlı paketlere ayrıldı. Ana exe yalnızca
**5.175 bayt katalog** taşır; kelime verileri ve çeviri modelleri içermez.
Kaynak SQLite/JSONL boyutu kullanıcıya gönderilen boyut değildir. Hazırlanan
ZIP paketlerinin lisans ve kaynak bildirimleri dahil toplamı **19.769.702 bayt
(19,77 MB)**. MB değerleri ondalıktır.

| Dil paketi | İndirme toplamı | Açık TSV veri |
| --- | ---: | ---: |
| İngilizce ↔ Türkçe | 1,13 MB | 3,22 MB |
| İngilizce ↔ İspanyolca | 2,22 MB | 7,07 MB |
| İngilizce ↔ Almanca | 3,65 MB | 11,30 MB |
| İngilizce ↔ Fransızca | 4,02 MB | 13,67 MB |
| İngilizce ↔ Japonca | 1,58 MB | 4,31 MB |
| İngilizce ↔ Çince | 5,91 MB | 16,40 MB |
| Korece → İngilizce | 1,26 MB | 3,36 MB |

Yönler bağımsız indirilir. Örneğin yalnızca İngilizce→Türkçe **0,63 MB**;
Türkçe→İngilizce **0,50 MB**. En büyük yön Çince→İngilizce **5,52 MB**.
Dosyalar `%LOCALAPPDATA%\simPl\dictionaries` altında sıkıştırılmış ZIP olarak
saklanır; yeniden açılışta internet gerekmez. Yalnızca etkin yön açılır ve
bellekte indekslenir. Açık TSV boyutu RAM kullanımını belirtmez; dizeler ve
arama indeksi de yer kaplar.

Ayarlar seçili yönü, paket boyutunu ve durumunu gösterir. Manage dictionaries
13 yönü açar; Download/Remove, ilerleme/Cancel, hata sonrası yeniden deneme ve
Import ZIP bulunur. Eksik sözlükte kelime kartı da indirme sunar; başarılı
kurulum aynı açık kartın sonucunu yeniler. Otomatik kelime araması otomatik
indirme başlatmaz. Seçili metin veya belge içeriği internete gönderilmez.

Paketler ayrı GitHub veri sürümünden HTTPS ile alınır. Boyut ve SHA-256,
exe içindeki değişmez katalogla doğrulanır; TSV ve manifest de doğrulanır.
Dosyadan ekleme aynı doğrulama yolunu kullanır. İptal veya bozuk indirme mevcut
geçerli paketi değiştirmez. Bir veri sürümünün dosyaları yerinde değiştirilmemeli;
yeni sürüm farklı ad ve güncel uygulama kataloğu gerektirir. Genel üçüncü taraf
sözlük biçimleri ve Argos motoru/model paketleri sonraki işlerdir.

Önceki gömülü veri build'inde exe **38.008.320 bayt (38,01 MB)**, PDFium dahil
portable klasör **47.591.520 bayt (47,59 MB)**, portable ZIP **31.489.574 bayt
(31,49 MB)** idi. Yeni indirilebilir paket build'inde exe **18,444,288 bayt (18.44 MB)**,
portable klasör **28,033,234 bayt (28.03 MB)**, portable ZIP
**11,862,130 bayt (11.86 MB)**. Release ve portable exe SHA-256 eşleşti;
ZIP CRC kontrolü geçti. Sözlük ZIP'leri normal portable paketine kopyalanmadı.
Bunlar installer boyutu değildir.
