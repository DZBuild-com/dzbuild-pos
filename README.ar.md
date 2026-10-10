<p align="center">
  <a href="README.md">English</a> · <a href="README.fr.md">Français</a> · <b>العربية</b>
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/dzbuild-pos-logo-horizontal-on-dark.png">
    <img src="assets/brand/dzbuild-pos-logo-horizontal.png" alt="DZBuild POS" width="420">
  </picture>
</p>

<div dir="rtl">

<h3 align="center">برنامج مجاني للبيع وتسيير المخزون للمحلات الجزائرية، يعمل دون إنترنت.</h3>

<p align="center">
  <a href="https://github.com/DZBuild-com/dzbuild-pos/actions/workflows/ci.yml"><img src="https://github.com/DZBuild-com/dzbuild-pos/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI"></a>
  <a href="https://github.com/DZBuild-com/dzbuild-pos/releases"><img src="https://img.shields.io/github/v/release/DZBuild-com/dzbuild-pos?label=release&color=2EA043" alt="Release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0--or--later-blue" alt="GPL-3.0-or-later"></a>
  <img src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078D6" alt="Windows 10 | 11">
  <img src="https://img.shields.io/badge/languages-ar%20%C2%B7%20fr%20%C2%B7%20en-FFC300" alt="العربية، الفرنسية، الإنجليزية">
</p>

<p align="center">
  <img src="assets/screenshots/settings-ar.png" alt="صفحة الإعدادات في DZBuild POS، واجهة عربية، مظهر داكن" width="820">
</p>

DZBuild POS برنامج لويندوز موجّه للمحلات في الجزائر: الصندوق، المخزون، الكريدي، الموردون، والفواتير
المطابقة للقانون الجزائري. الواجهة والوثائق المطبوعة متوفرة بالعربية والفرنسية والإنجليزية. يعمل دون
إنترنت ودون اشتراك.

## التحميل

المثبّتات متوفرة على [dzbuild.com/pos](https://dzbuild.com/pos) وفي صفحة
[GitHub Releases](https://github.com/DZBuild-com/dzbuild-pos/releases).

| المثبّت | لمن |
| --- | --- |
| 64 بت | أغلب الحواسيب (ويندوز 10 أو 11، 64 بت). يثبّت WebView2 إن لم يكن موجودًا. |
| 32 بت | الحواسيب القديمة بويندوز 10 نسخة 32 بت. |
| دون إنترنت (64 أو 32 بت) | الحواسيب غير المتصلة. يحتوي على WebView2 ويمكن نقله على مفتاح USB. |

مع كل إصدار ملف `SHA256SUMS.txt` للتحقق من الملف المحمَّل.

> [!NOTE]
> المثبّتات غير موقّعة بعد، لذا قد يعرض ويندوز رسالة «حمى Windows جهاز الكمبيوتر». اضغط
> **معلومات إضافية** ثم **التشغيل على أي حال** فقط إذا كان الملف من dzbuild.com/pos أو من هذا المستودع
> وتطابقت بصمته SHA-256.

## لماذا هو مجاني

تطوّر DZBuild هذا البرنامج، وهي أيضًا صاحبة منصة المتاجر الإلكترونية [DZBuild](https://dzbuild.com).
البرنامج مجاني: دون مفتاح ترخيص، دون اشتراك، دون حساب. كل الخصائص تعمل دون إنترنت.

المحل الذي يبيع أيضًا عبر الإنترنت يمكنه ربط الصندوق بمتجر DZBuild (الكتالوج، المخزون، الطلبيات). هذا
الربط اختياري.

## الخصائص

- **الصندوق**: البيع بلوحة المفاتيح وقارئ الباركود، الدفع بعدة طرق، حساب حق الطابع والباقي مباشرة،
  التذاكر المعلّقة، الإرجاع والاستبدال، حصص الصندوق حتى تقرير Z.
- **المخزون**: سجل حركات لا يُعدَّل، الجرد، التحويلات، الحصص وتواريخ انتهاء الصلاحية، الأرقام
  التسلسلية وIMEI، التكلفة المتوسطة المرجّحة.
- **الزبائن والكريدي**: البيع بالكريدي مع سقف، الدفعات بوصل مرقّم، كشوف الحساب، نقاط الولاء والقسائم.
- **المشتريات**: طلبيات الشراء، الاستلام على دفعات، فواتير الموردين وتسديداتها، المرتجعات.
- **الوثائق والجباية**: عروض الأسعار، الفواتير الشكلية، وصولات التسليم، الفواتير وفواتير الإلغاء بترقيم
  متسلسل، البيانات الإلزامية للمرسوم 05-468، سجل تدقيق مسلسل، 60 تقريرًا منها TVA والطابع ومساعد G50.
- **الطباعة والعتاد**: طابعات التذاكر ESC/POS، فواتير A4 وA5 وPDF، طابعات الملصقات TSPL وZPL،
  الموازين، درج النقود، شاشة الزبون.
- **عدة صناديق**: حاسوب رئيسي وصناديق على الشبكة المحلية. يواصل الصندوق البيع إذا انقطعت الشبكة.
- **الاستيراد والتصدير**: Excel (xlsx وxls وods) وCSV، بما فيها ترميزات ويندوز القديمة.
- **نماذج النشاط**: 25 نموذجًا (مطعم، صيدلية، جزارة، هواتف، بيع بالجملة، مجوهرات، وغيرها).

**ما لا يفعله**: لا فواتير وهمية، لا مخزون مخفي، لا حذف للمبيعات، فهذا ممنوع قانونًا في الجزائر. يُصحَّح
الخطأ بفاتورة إلغاء أو بإرجاع.

## المساهمة

يتولى فريق DZBuild صيانة المشروع. المساهمات في البرمجة والترجمة وتجارب العتاد مرحّب بها: اقرأ
[CONTRIBUTING.md](CONTRIBUTING.md) (بالإنجليزية)، واطرح أسئلتك في
[Discussions](https://github.com/DZBuild-com/dzbuild-pos/discussions). للتبليغ عن ثغرة أمنية اتبع
[SECURITY.md](SECURITY.md) ولا تفتح تذكرة علنية.

## الرخصة

برنامج حر تحت رخصة [GNU GPL الإصدار 3 أو أحدث](LICENSE). «DZBuild» و«DZBuild POS» والشعار علامات
تجارية لـ DZBuild غير مشمولة بالرخصة: انظر [TRADEMARKS.md](TRADEMARKS.md).

روابط: [dzbuild.com/pos](https://dzbuild.com/pos) · [المساعدة](https://dzbuild.com/pos/support) ·
[المدونة](https://blog.dzbuild.com) · [أنشئ متجر DZBuild](https://dzbuild.com/register)

</div>
