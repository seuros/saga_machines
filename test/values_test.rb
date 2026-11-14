# frozen_string_literal: true

require_relative 'test_helper'

# A value built by hand is held to the rule a returned one is.
class ValuesTest < Minitest::Test
  include SagaHelpers

  def test_ids_validate
    [0, 1 << 48, 65_535 << 48, -1, 2**64].each { |raw| assert_raises(RangeError) { AttemptId.new(raw) } }
    assert_equal 7, AttemptId.new((7 << 48) | 1).scope
    [-1, 2**64].each { |raw| assert_raises(RangeError) { SagaId.new(raw) } }
    [1.0, nil, '1'].each do |bad|
      assert_raises(TypeError) { AttemptId.new(bad) }
      assert_raises(TypeError) { SagaId.new(bad) }
    end
  end

  def test_keys_actions_and_progress_validate
    key = ActionKey.new(ID, 0, :forward)
    assert_raises(TypeError) { ActionKey.new(7, 0, :forward) }
    assert_raises(TypeError) { ActionKey.new(ID, 0.0, :forward) }
    assert_raises(RangeError) { ActionKey.new(ID, -1, :forward) }
    assert_raises(TypeError) { ActionKey.new(ID, 0, 'forward') }
    assert_raises(RangeError) { ActionKey.new(ID, 0, :sideways) }
    assert_raises(TypeError) { Action.new(key, 1) }
    assert_raises(TypeError) { Action.new([7, 0, :forward], AttemptId.new(1)) }
    [Forward, Compensating].each do |variant|
      assert_raises(TypeError) { variant.new(nil) }
      assert_raises(RangeError) { variant.new(-1) }
      assert_equal 3, variant.new(3).step
    end
  end
end
