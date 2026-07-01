# BudgetCut'ı Dene 🎬

Film/dizi bütçeleme uygulaması. **Offline çalışır, kurulum gerektirmez** — sadece aç ve dene.

## Kurulum (macOS)

1. `BudgetCut_0.1.0_aarch64.dmg` dosyasını çift tıkla, **BudgetCut**'ı `Applications`'a sürükle.
2. İlk açılışta macOS imzasız uygulamayı uyarır:
   **Uygulamaya sağ tık → "Aç" → "Aç"** (bir kez; sonra normal açılır).
   _(Apple Developer imzası yok; bu yüzden ilk açılışta sağ-tık-Aç gerekir.)_

İlk açılışta örnek bir **Türk dizisi bütçesi** (Netflix hesap kodlarıyla, 1. Bölüm) hazır gelir.

## Ne denenir?

- **Özet (Topsheet):** kategori bazında ATL/BTL, yansıtmalar, NET TOPLAM. Üstteki **Çekim günü**
  alanını değiştir → güne bağlı satırlar (ekip, kamera) anında yeniden hesaplanır.
- **Hesap Detayları:** satırları **düzenle** (açıklama, adet, birim tutar — hücreye tıkla),
  **+ Açıklama** ile yeni satır ekle, **×** ile sil. Net/Yansıtma/G.Toplam otomatik güncellenir.
  - İpucu: bir tutar alanına `=CEKIM_GUN` gibi yazarsan o satır global değişkene bağlanır.
- **Kurulum Araçları:** Türk bordro yansıtmaları — **Stopaj** brüte-tamamlama (gross-up),
  **SGK/Komisyon** ek (additive). Global değişkenler, birimler.
- **⌘K:** komut paleti (görünüm değiştir, dil TR/EN).

Tüm değişiklikler diske kaydedilir (SQLite); uygulamayı kapatıp açınca aynen durur.

## Rakamlar nasıl hesaplanıyor?

Bütün matematik Rust çekirdeğinde (`budgetcut-core`): para `decimal` (kuruş hassas),
stopaj `brüt = net / (1 − oran)`, komisyon `net × oran`. Gerçek bir dizi bütçesindeki
rakamlarla birebir doğrulanmış.

## Geri bildirim

Takıldığın/garip bulduğun her şeyi not al — özellikle: hesaplama doğru mu, düzenleme akıcı mı,
Türkçe terimler yerinde mi.

---
İleride: gerçek-zamanlı çoklu kullanıcı (sunucu hazır), MMB içe aktarma, planlama, mobil.
