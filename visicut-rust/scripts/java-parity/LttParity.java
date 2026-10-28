/*
 * Reference generator for the Java-parity tests of VisiCutR.
 *
 * Runs each case of visicut-rust/tests/java_parity through the original
 * Java pipeline (VisiCut SVG import, VectorProfile/RasterProfile and the
 * LibLaserCut LaserToolsTechnicsCutter of the FAU device) and writes the
 * resulting LTT bytes as expected.ltt next to the case. Started by
 * generate.sh; see the README in this folder.
 *
 * LGPL-3.0-or-later, like VisiCut and LibLaserCut.
 */

import de.thomas_oster.liblasercut.LaserCutter;
import de.thomas_oster.liblasercut.properties.LaserProperty;
import de.thomas_oster.liblasercut.dithering.DitheringAlgorithm;
import de.thomas_oster.visicut.VisicutModel;
import de.thomas_oster.visicut.managers.LaserDeviceManager;
import de.thomas_oster.visicut.model.LaserDevice;
import de.thomas_oster.visicut.model.LaserProfile;
import de.thomas_oster.visicut.model.PlfPart;
import de.thomas_oster.visicut.model.Raster3dProfile;
import de.thomas_oster.visicut.model.RasterProfile;
import de.thomas_oster.visicut.model.VectorProfile;
import de.thomas_oster.visicut.model.mapping.Mapping;
import de.thomas_oster.visicut.model.mapping.MappingSet;
import java.awt.geom.AffineTransform;
import java.io.File;
import java.io.FileInputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.LinkedList;
import java.util.List;
import java.util.Map;
import java.util.Properties;

public class LttParity
{
  public static void main(String[] args) throws Exception
  {
    if (args.length < 2)
    {
      System.err.println("usage: LttParity <device.xml> <case-dir>...");
      System.exit(2);
    }
    LaserDevice device = LaserDeviceManager.getInstance().loadFromFile(new File(args[0]));
    int failed = 0;
    for (int i = 1; i < args.length; i++)
    {
      File dir = new File(args[i]);
      Properties c = new Properties();
      try (InputStreamReader in = new InputStreamReader(new FileInputStream(new File(dir, "case.properties")), StandardCharsets.UTF_8))
      {
        c.load(in);
      }
      for (Map.Entry<String, Properties> variant : variants(c).entrySet())
      {
        String suffix = variant.getKey();
        try
        {
          generate(device, dir, variant.getValue(), suffix);
        }
        catch (Throwable t)
        {
          // A case Java cannot process: record the reason instead of stopping.
          failed++;
          new File(dir, "expected" + suffix + ".ltt").delete();
          StringBuilder reason = new StringBuilder();
          for (Throwable cause = t; cause != null; cause = cause.getCause())
          {
            if (reason.length() > 0)
            {
              reason.append(" <- ");
            }
            reason.append(cause.getClass().getSimpleName()).append(": ").append(cause.getMessage());
          }
          java.nio.file.Files.writeString(new File(dir, "expected" + suffix + ".error").toPath(), reason + "\n");
          System.out.println(dir.getName() + suffix + ": JAVA ERROR " + reason);
        }
      }
    }
    if (failed > 0)
    {
      System.out.println(failed + " case(s) failed in Java");
    }
  }

  /**
   * The case itself (suffix "") and each of its "variants", a comma-separated
   * list of key=value overrides such as "order=FILE, rotary_diameter_mm=100";
   * a variant writes expected@key-value.ltt (see tests/java_parity.rs).
   */
  private static Map<String, Properties> variants(Properties c)
  {
    Map<String, Properties> all = new java.util.LinkedHashMap<>();
    all.put("", c);
    for (String v : c.getProperty("variants", "").split(","))
    {
      String[] kv = v.trim().split("=", 2);
      if (kv.length != 2)
      {
        continue;
      }
      Properties p = (Properties) c.clone();
      p.setProperty(kv[0].trim(), kv[1].trim());
      all.put("@" + kv[0].trim() + "-" + kv[1].trim(), p);
    }
    return all;
  }

  private static void generate(LaserDevice device, File dir, Properties c, String suffix) throws Exception
  {
    LaserCutter cutter = device.getLaserCutter();
    String operation = c.getProperty("operation");
    LaserProfile profile;
    LaserProperty property;
    String prefix;
    switch (operation)
    {
      case "cut":
      case "mark":
      {
        VectorProfile vp = new VectorProfile();
        vp.setIsCut("cut".equals(operation));
        // FILE, NEAREST, INNER_FIRST (default) or SMALLEST_FIRST.
        vp.setOrderStrategy(de.thomas_oster.liblasercut.vectoroptimizers.VectorOptimizer.OrderStrategy.valueOf(c.getProperty("order", "INNER_FIRST")));
        // "Nur Umriss": only the outline of all objects together.
        vp.setUseOutline(Boolean.parseBoolean(c.getProperty("outline", "false")));
        profile = vp;
        property = cutter.getLaserPropertyForVectorPart();
        prefix = "cut".equals(operation) ? "Cut" : "Mark";
        break;
      }
      case "engrave":
      {
        RasterProfile rp = new RasterProfile();
        // Class name in de.thomas_oster.liblasercut.dithering, e.g. FloydSteinberg.
        rp.setDitherAlgorithm((DitheringAlgorithm) Class.forName("de.thomas_oster.liblasercut.dithering." + c.getProperty("dither", "FloydSteinberg")).getDeclaredConstructor().newInstance());
        rp.setColorShift(Integer.parseInt(c.getProperty("color_shift", "0")));
        rp.setInvertColors(Boolean.parseBoolean(c.getProperty("invert", "false")));
        profile = rp;
        property = cutter.getLaserPropertyForRasterPart();
        property.setProperty("engrave unidirectional", Boolean.parseBoolean(c.getProperty("unidirectional", "false")));
        property.setProperty("engrave bottom up", Boolean.parseBoolean(c.getProperty("bottom_up", "false")));
        prefix = "Engrav";
        break;
      }
      case "engrave3d":
      {
        profile = new Raster3dProfile();
        property = cutter.getLaserPropertyForRaster3dPart();
        prefix = "Eng3D";
        break;
      }
      default:
        throw new IllegalArgumentException(dir + ": unknown operation " + operation);
    }
    profile.setDPI(500);
    property.setProperty("power", Float.parseFloat(c.getProperty("power")));
    property.setProperty("speed", Float.parseFloat(c.getProperty("speed")));
    // VisiCut has no pass counter: every further LaserProperty of a profile
    // processes the same objects once more.
    List<LaserProperty> props = new LinkedList<>();
    int passes = Integer.parseInt(c.getProperty("passes", "1"));
    for (int i = 0; i < passes; i++)
    {
      props.add(property.clone());
    }
    // Further parameter sets: "power:speed:passes;power:speed:passes".
    for (String set : c.getProperty("additional", "").split(";"))
    {
      if (set.isBlank())
      {
        continue;
      }
      String[] v = set.trim().split(":");
      LaserProperty p = property.clone();
      p.setProperty("power", Float.parseFloat(v[0]));
      p.setProperty("speed", Float.parseFloat(v[1]));
      for (int i = 0; i < Integer.parseInt(v[2]); i++)
      {
        props.add(p.clone());
      }
    }

    VisicutModel model = new VisicutModel();
    model.setSelectedLaserDevice(device);
    // Added to the focus of every LaserProperty (useThicknessAsFocusOffset).
    model.setMaterialThickness(Float.parseFloat(c.getProperty("thickness_mm", "0")));
    // Rotary engraving: Y becomes the rotation of a cylinder of this diameter.
    if (c.getProperty("rotary_diameter_mm") != null)
    {
      model.setRotaryAxisEnabled(true);
      model.setRotaryAxisDiameterMm(Double.parseDouble(c.getProperty("rotary_diameter_mm")));
    }
    List<String> warnings = new ArrayList<>();
    PlfPart part = model.loadGraphicFile(new File(dir, c.getProperty("svg")), warnings);
    double x = Double.parseDouble(c.getProperty("x_mm"));
    double y = Double.parseDouble(c.getProperty("y_mm"));
    // "place=bbox": the top left corner of the geometry (without strokes)
    // goes to x/y, like placing a part in the GUI, instead of the document origin.
    if ("bbox".equals(c.getProperty("place")))
    {
      java.awt.geom.Rectangle2D bounds = geometryBounds(part.getGraphicObjects());
      x -= bounds.getX();
      y -= bounds.getY();
    }
    AffineTransform t = AffineTransform.getTranslateInstance(x, y);
    // Optional "scale": the part is resized like in VisiCut's GUI (uniform).
    double scale = Double.parseDouble(c.getProperty("scale", "1"));
    if (scale != 1)
    {
      t.scale(scale, scale);
    }
    t.concatenate(part.getGraphicObjects().getBasicTransform());
    // Natural size of the import in mm, for cases that let both sides derive it.
    java.awt.geom.Rectangle2D natural = part.getGraphicObjects().getBoundingBox();
    System.out.println(dir.getName() + ": java import bounds " + natural);
    part.getGraphicObjects().setTransform(t);
    // Profiles compare equal by their settings; each mapping needs its own.
    Map<LaserProfile, List<LaserProperty>> propmap = new java.util.IdentityHashMap<>();
    MappingSet mapping = new MappingSet();
    String mappings = c.getProperty("mappings");
    if (mappings == null)
    {
      mapping.add(new Mapping(null, profile));
      propmap.put(profile, props);
    }
    else
    {
      // "filters -> power:speed[:passes]; …" in mapping order, see README.
      for (String entry : mappings.split(";"))
      {
        String[] parts = entry.split("->");
        String[] v = parts[1].trim().split(":");
        LaserProfile mapped = profile.clone();
        LaserProperty p = property.clone();
        p.setProperty("power", Float.parseFloat(v[0]));
        p.setProperty("speed", Float.parseFloat(v[1]));
        List<LaserProperty> mappedProps = new LinkedList<>();
        for (int i = 0; i < (v.length > 2 ? Integer.parseInt(v[2]) : 1); i++)
        {
          mappedProps.add(p.clone());
        }
        mapping.add(new Mapping(filterSet(parts[0].trim()), mapped));
        propmap.put(mapped, mappedProps);
      }
    }
    part.setMapping(mapping);
    model.getPlfFile().add(part);

    String name = jobName(prefix, c.getProperty("name"));
    File out = new File(dir, "expected" + suffix + ".ltt");
    new File(dir, "expected" + suffix + ".error").delete();
    model.saveJob(name, out, null, propmap);
    for (String w : warnings)
    {
      System.err.println(dir.getName() + ": " + w);
    }
    System.out.println(dir.getName() + suffix + ": " + out.length() + " bytes as " + name);
  }

  /**
   * "*": everything else (no FilterSet); "all": everything (empty FilterSet);
   * otherwise filters joined by "&", each "[!]Attribute=value" or
   * "[!]Attribute<=value" with VisiCut's attribute names. Colours become
   * java.awt.Color, stroke widths Double, like SVGShape's attribute values.
   */
  private static de.thomas_oster.visicut.model.mapping.FilterSet filterSet(String spec)
  {
    if ("*".equals(spec))
    {
      return null;
    }
    de.thomas_oster.visicut.model.mapping.FilterSet set = new de.thomas_oster.visicut.model.mapping.FilterSet();
    if ("all".equals(spec))
    {
      return set;
    }
    for (String f : spec.split("&"))
    {
      f = f.trim();
      boolean inverted = f.startsWith("!");
      if (inverted)
      {
        f = f.substring(1);
      }
      boolean compare = f.contains("<=");
      String[] kv = f.split(compare ? "<=" : "=", 2);
      String attribute = kv[0].trim();
      String text = kv[1].trim();
      Object value;
      if (attribute.equals("Stroke Width"))
      {
        value = Double.parseDouble(text);
      }
      else if (attribute.endsWith("Color") && text.startsWith("#"))
      {
        value = java.awt.Color.decode(text);
      }
      else
      {
        value = text;
      }
      de.thomas_oster.visicut.model.mapping.MappingFilter filter = new de.thomas_oster.visicut.model.mapping.MappingFilter(attribute, value);
      filter.setInverted(inverted);
      filter.setCompare(compare);
      set.add(filter);
    }
    return set;
  }

  /** Union of the objects' geometry bounds in mm (basic transform, no strokes). */
  private static java.awt.geom.Rectangle2D geometryBounds(de.thomas_oster.visicut.model.graphicelements.GraphicSet objects)
  {
    java.awt.geom.Rectangle2D result = null;
    for (de.thomas_oster.visicut.model.graphicelements.GraphicObject e : objects)
    {
      java.awt.Shape shape = (e instanceof de.thomas_oster.visicut.model.graphicelements.ShapeObject)
        ? ((de.thomas_oster.visicut.model.graphicelements.ShapeObject) e).getShape() : e.getBoundingBox();
      java.awt.geom.Rectangle2D bb = objects.getBasicTransform().createTransformedShape(shape).getBounds2D();
      result = result == null ? bb : result.createUnion(bb);
    }
    return result == null ? new java.awt.geom.Rectangle2D.Double() : result;
  }

  /** Same device job name as VisiCutR (prefix, ASCII, at most 15 characters). */
  private static String jobName(String prefix, String name)
  {
    StringBuilder b = new StringBuilder();
    for (char ch : (prefix + "_" + name).toCharArray())
    {
      if (b.length() == 15)
      {
        break;
      }
      boolean ok = (ch < 128 && Character.isLetterOrDigit(ch)) || ch == '-' || ch == '_';
      b.append(ok ? ch : '_');
    }
    return b.toString();
  }
}
